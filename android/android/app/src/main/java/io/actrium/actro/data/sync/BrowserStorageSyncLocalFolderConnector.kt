package io.actrium.actro.data.sync

import android.content.Context
import android.util.Log
import io.actrium.actro.rust.ActroCore
import java.io.File
import java.io.IOException
import java.util.UUID
import org.json.JSONException
import org.json.JSONObject

// Outcome of the local-folder acceptance pipeline. blockedReasonKey is a stable
// token the settings UI can map to a localized string; detailText carries the
// blocking stage's action / classification token so the error can say what to
// do, not just that acceptance failed.
data class BrowserStorageSyncFolderPreflightResult(
    val accept: Boolean,
    val blockedReasonKey: String?,
    val detailText: String
)

// The persisted result of a connect attempt: on accept the settings snapshot
// reflects the newly configured folder; on block it is the pre-attempt
// snapshot so the caller can still report the current (unchanged) provider.
data class BrowserStorageSyncConnectOutcome(
    val accepted: Boolean,
    val blockedReasonKey: String?,
    val detailText: String,
    val settings: BrowserStorageSyncProviderSettings
)

/**
 * The truly minimal, Rust-compatible local-folder entry that mirrors the
 * validated Windows engine wiring without any permission surface.
 *
 * Android's modern shared-folder picker (`ACTION_OPEN_DOCUMENT_TREE`) returns a
 * `content://` tree Uri, not a filesystem path, and the Rust local-folder
 * adapter operates on a real path via `std::fs` -- so a SAF tree cannot feed
 * the sync rounds and belongs to the product-gated cloud-provider work. Instead
 * this uses the app-scoped external directory (`getExternalFilesDir`), a real
 * absolute path that needs no runtime permission, and gates acceptance through
 * the same Rust capability / adapter / compatibility preflights the Windows
 * accept path uses. Nothing is persisted until every gate passes.
 */
object BrowserStorageSyncLocalFolderConnector {
    const val FOLDER_MISSING_REASON = "folderMissing"
    const val FOLDER_NOT_WRITABLE_REASON = "folderNotWritable"
    const val CAPABILITY_BLOCKED_REASON = "capabilityBlocked"
    const val ADAPTER_BLOCKED_REASON = "adapterBlocked"
    const val COMPATIBILITY_BLOCKED_REASON = "compatibilityBlocked"

    private const val TAG = "BrowserSyncFolderConnect"
    private const val SUBDIR = "browser-sync"

    fun defaultRootDir(context: Context): File {
        val appContext = context.applicationContext
        val base = appContext.getExternalFilesDir(null) ?: appContext.filesDir
        return File(base, SUBDIR)
    }

    // Runs the accept pipeline over the app-scoped external folder and, only on
    // pass, persists it as the active local-folder provider. The Rust round
    // input is the real absolute path; each later sync round re-validates the
    // folder authoritatively through its own preflights.
    fun connect(
        context: Context,
        store: BrowserStorageSyncProviderSettingsStore,
        observedAtUnixSeconds: Long = System.currentTimeMillis() / 1000
    ): BrowserStorageSyncConnectOutcome {
        val root = defaultRootDir(context)
        root.mkdirs()
        val directoryExists = root.isDirectory
        val rootPath = root.absolutePath
        val result = evaluate(
            directoryExists = directoryExists,
            writeProbeSucceeded = directoryExists && runWriteProbe(root),
            capabilityPreflightJson =
                ActroCore.browserStorageSyncCloudLocalFolderCapabilityPreflightJson(observedAtUnixSeconds),
            adapterPlanJson = ActroCore.browserStorageSyncCloudProviderAdapterPlanJson(
                adapterSnapshotJson = ActroCore.browserStorageSyncCloudLocalFolderCapabilitiesJson(),
                observedAtUnixSeconds = observedAtUnixSeconds,
            ),
            compatibilityPreflightJson =
                ActroCore.browserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
                    rootDir = rootPath,
                    observedAtUnixSeconds = observedAtUnixSeconds,
                ),
        )

        if (!result.accept) {
            Log.w(
                TAG,
                "Local-folder browser sync connect blocked at ${result.blockedReasonKey} " +
                    "(${result.detailText}) for $rootPath",
            )
            return BrowserStorageSyncConnectOutcome(
                accepted = false,
                blockedReasonKey = result.blockedReasonKey,
                detailText = result.detailText,
                settings = store.load(),
            )
        }

        val snapshot = store.configureLocalFolder(rootPath)
        Log.i(TAG, "Local-folder browser sync provider configured at $rootPath")
        return BrowserStorageSyncConnectOutcome(
            accepted = true,
            blockedReasonKey = null,
            detailText = "",
            settings = snapshot,
        )
    }

    // Accept gating for the local-folder provider connection, mirroring the
    // Windows BrowserSyncFolderPreflight. The stages run in cost order --
    // platform probe (IO booleans arrive here so this stays pure), provider
    // capability gate, object-space adapter plan, and the cloud-bytes
    // compatibility gate over whatever already lives in the folder. The Rust
    // core owns every classification; this only sequences the gates and
    // extracts the blocking token.
    fun evaluate(
        directoryExists: Boolean,
        writeProbeSucceeded: Boolean,
        capabilityPreflightJson: String,
        adapterPlanJson: String,
        compatibilityPreflightJson: String
    ): BrowserStorageSyncFolderPreflightResult {
        // 1. Platform probe. A folder that does not exist or rejects the write
        //    probe can never host the sync store, so the contract gates are not
        //    even consulted.
        if (!directoryExists) {
            return blocked(FOLDER_MISSING_REASON, "")
        }
        if (!writeProbeSucceeded) {
            return blocked(FOLDER_NOT_WRITABLE_REASON, "")
        }

        // 2. Provider capability gate (ready / degraded / setup-required). An
        //    invalid plan is contract drift and blocks acceptance rather than
        //    letting an unvalidated folder into the settings store.
        val capabilityPreflight = capabilityPreflightJson.toJsonObjectOrNull()
        if (capabilityPreflight == null ||
            capabilityPreflight.optBoolean("shouldAllowSync", false) != true ||
            capabilityPreflight.optBoolean("shouldBlockSync", false)
        ) {
            return blocked(CAPABILITY_BLOCKED_REASON, capabilityPreflight.optStringOrEmpty("classification"))
        }

        // 3. Object-space adapter plan: sync must be allowed; otherwise the
        //    rootPathAction (or the classification when no root action is named)
        //    is surfaced as the thing to fix.
        val adapterPlan = adapterPlanJson.toJsonObjectOrNull()
        if (adapterPlan == null || adapterPlan.optBoolean("shouldAllowSync", false) != true) {
            val rootPathAction = adapterPlan.optStringOrEmpty("rootPathAction")
            return blocked(
                ADAPTER_BLOCKED_REASON,
                rootPathAction.ifEmpty { adapterPlan.optStringOrEmpty("classification") },
            )
        }

        // 4. Cloud-bytes compatibility gate: a clientUpgradeRequired or invalid
        //    outcome blocks acceptance with the policy's user action rendered as
        //    the error detail.
        val compatibilityPreflight = compatibilityPreflightJson.toJsonObjectOrNull()
        if (compatibilityPreflight == null || compatibilityPreflight.optBoolean("shouldBlockSync", false)) {
            val userAction = compatibilityPreflight.optStringOrEmpty("userAction")
            return blocked(
                COMPATIBILITY_BLOCKED_REASON,
                if (userAction.isNotEmpty() && userAction != "none") {
                    userAction
                } else {
                    compatibilityPreflight.optStringOrEmpty("classification")
                },
            )
        }

        return BrowserStorageSyncFolderPreflightResult(accept = true, blockedReasonKey = null, detailText = "")
    }

    // The write probe is IO by design: the evaluator above stays pure, and each
    // sync round re-validates the folder authoritatively through its own
    // preflights.
    private fun runWriteProbe(root: File): Boolean {
        val probe = File(root, ".actro-sync-write-probe-${UUID.randomUUID()}")
        return try {
            probe.writeText("probe")
            probe.delete()
            true
        } catch (error: IOException) {
            Log.w(TAG, "Local-folder browser sync write probe failed for ${root.absolutePath}", error)
            false
        } catch (error: SecurityException) {
            Log.w(TAG, "Local-folder browser sync write probe denied for ${root.absolutePath}", error)
            false
        }
    }

    private fun blocked(reasonKey: String, detailText: String): BrowserStorageSyncFolderPreflightResult =
        BrowserStorageSyncFolderPreflightResult(
            accept = false,
            blockedReasonKey = reasonKey,
            detailText = detailText,
        )
}

private fun String.toJsonObjectOrNull(): JSONObject? =
    try {
        if (this == "null") null else JSONObject(this)
    } catch (_: JSONException) {
        null
    }

private fun JSONObject?.optStringOrEmpty(name: String): String =
    this?.optString(name).orEmpty()
