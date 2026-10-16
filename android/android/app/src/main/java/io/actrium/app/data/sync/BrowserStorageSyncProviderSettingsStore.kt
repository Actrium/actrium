package io.actrium.app.data.sync

import android.content.Context
import android.util.Log
import java.io.File
import java.io.IOException
import java.util.UUID
import org.json.JSONException
import org.json.JSONObject

// Versioned browser sync provider connection settings, the Android sibling of
// the Windows `browser-sync-provider.json` store and the macOS
// BrowserSyncProviderSettingsSnapshot. The snapshot intentionally never
// contains a credential field: the local-folder provider has no secrets, and
// later cloud providers keep their tokens in provider-SDK / OS storage, so the
// file stays plain JSON with no encryption.
data class BrowserStorageSyncProviderSettings(
    val version: Int,
    val providerKind: String?,
    val localFolderRootPath: String?,
    val scope: String,
    val deviceId: String,
    val membershipJoinedAtUnixSeconds: Long?,
    val savedAtUnixSeconds: Long
) {
    val isConfigured: Boolean
        get() {
            val kind = providerKind
            if (kind.isNullOrEmpty()) {
                return false
            }
            return kind != BrowserStorageSyncProviderSettingsStore.LOCAL_FOLDER_PROVIDER_KIND ||
                !localFolderRootPath.isNullOrEmpty()
        }
}

/**
 * Owner of the durable browser sync provider connection settings on Android.
 *
 * This store, not the endpoint credential store, is the device-identity owner:
 * `deviceId` is minted / persisted here, adopting the legacy endpoint device
 * id through [deviceIdProvider] the first time this file is written so the same
 * machine never appears as two cloud devices. The endpoint credential store
 * keeps its own copy of the id independently, which lets a corrupt or missing
 * provider-settings file recover the same identity instead of minting a ghost.
 *
 * `membershipJoinedAtUnixSeconds` is the persisted connection outcome: without
 * it every cold start would re-run the membership round (a write to the shared
 * cloud folder). A stale-but-wrong marker costs exactly one corrective round
 * because the coordinator clears it on membership- or storage-scoped provider
 * errors.
 */
class BrowserStorageSyncProviderSettingsStore(
    private val file: File,
    private val deviceIdProvider: () -> String
) {
    constructor(context: Context, deviceIdProvider: () -> String) : this(
        File(context.applicationContext.filesDir, FILE_NAME),
        deviceIdProvider,
    )

    @Synchronized
    fun load(): BrowserStorageSyncProviderSettings {
        if (!file.exists()) {
            // First load: mint the durable device identity (normalize fills the
            // empty id from the credential store or a fresh UUID) and persist it
            // immediately so it survives restarts and later disconnects.
            return save(normalize(null))
        }

        val text = try {
            file.readText()
        } catch (error: IOException) {
            // An unreadable file must not crash sync. Device identity is also
            // held by the endpoint credential store, so normalize(null) adopts
            // the same id instead of losing it; report not-configured and log.
            Log.e(TAG, "Browser sync provider settings unreadable; treating as not configured", error)
            return normalize(null)
        }

        val parsed = try {
            JSONObject(text)
        } catch (error: JSONException) {
            Log.e(TAG, "Browser sync provider settings corrupt; treating as not configured", error)
            return normalize(null)
        }

        return normalize(parsed.toSettingsOrNull())
    }

    @Synchronized
    fun save(settings: BrowserStorageSyncProviderSettings): BrowserStorageSyncProviderSettings {
        val normalized = normalize(settings)
        try {
            file.parentFile?.mkdirs()
            file.writeText(normalized.toJson().toString(2))
        } catch (error: IOException) {
            Log.e(TAG, "Failed to persist browser sync provider settings", error)
        }
        return normalized
    }

    // Builds a snapshot over the current durable device id without persisting
    // it; the membership marker defaults to null because a new (or re-targeted)
    // provider connection has not joined the store yet.
    fun createSnapshot(
        providerKind: String?,
        localFolderRootPath: String?,
        scope: String? = null
    ): BrowserStorageSyncProviderSettings {
        val current = load()
        return normalize(
            current.copy(
                providerKind = providerKind,
                localFolderRootPath = localFolderRootPath,
                scope = scope ?: current.scope,
                membershipJoinedAtUnixSeconds = null,
                savedAtUnixSeconds = 0,
            ),
        )
    }

    fun configureLocalFolder(rootPath: String): BrowserStorageSyncProviderSettings =
        save(createSnapshot(LOCAL_FOLDER_PROVIDER_KIND, rootPath))

    fun saveMembershipJoined(joinedAtUnixSeconds: Long): BrowserStorageSyncProviderSettings =
        save(load().copy(membershipJoinedAtUnixSeconds = joinedAtUnixSeconds, savedAtUnixSeconds = 0))

    fun clearMembershipMarker(): BrowserStorageSyncProviderSettings =
        save(load().copy(membershipJoinedAtUnixSeconds = null, savedAtUnixSeconds = 0))

    // Disconnect means "stop provider sync": the provider fields and the
    // membership marker are cleared, but deviceId is durable identity and
    // stays, so a later reconnect resumes the same cloud device document.
    fun disconnect(): BrowserStorageSyncProviderSettings =
        save(
            load().copy(
                providerKind = null,
                localFolderRootPath = null,
                membershipJoinedAtUnixSeconds = null,
                savedAtUnixSeconds = 0,
            ),
        )

    // Disconnect means "stop browser sync entirely", not "fall back to the
    // reference server": clearing only the provider fields would silently
    // reactivate endpoint sync for anyone with leftover credentials from the
    // transitional dialog while the UI claims "not configured". Device identity
    // is unaffected -- this store owns it.
    fun disconnectAndClearEndpointSession(
        endpointCredentialStore: BrowserStorageSyncCredentialStore
    ): BrowserStorageSyncProviderSettings {
        val snapshot = disconnect()
        endpointCredentialStore.clearSession()
        return snapshot
    }

    private fun normalize(
        settings: BrowserStorageSyncProviderSettings?
    ): BrowserStorageSyncProviderSettings {
        val providerKind = normalizeProviderKind(settings?.providerKind)
        val rootPath = settings?.localFolderRootPath?.trim().orEmpty()
        // The local-folder root must be absolute: a relative path would resolve
        // against the process working directory and sync into the wrong folder,
        // so it degrades to "not configured" instead.
        val normalizedRootPath =
            if (providerKind == LOCAL_FOLDER_PROVIDER_KIND &&
                rootPath.isNotEmpty() &&
                File(rootPath).isAbsolute
            ) {
                rootPath
            } else {
                null
            }
        val configured = providerKind != null &&
            (providerKind != LOCAL_FOLDER_PROVIDER_KIND || normalizedRootPath != null)

        val scope = settings?.scope?.trim().orEmpty().ifEmpty { DEFAULT_SCOPE }
        // Device identity is adopted from the credential store (or a fresh UUID)
        // only when this file has no id yet; after the first save the persisted
        // id is reused and the credential store is not touched again.
        val deviceId = settings?.deviceId?.trim().orEmpty().ifEmpty {
            deviceIdProvider().trim().ifEmpty { UUID.randomUUID().toString() }
        }

        // A membership marker without a configured provider is meaningless and
        // would leak into the next connection.
        val membershipJoinedAt = settings?.membershipJoinedAtUnixSeconds
        val normalizedMembership =
            if (configured && (membershipJoinedAt ?: 0L) > 0L) membershipJoinedAt else null

        val savedAt =
            if ((settings?.savedAtUnixSeconds ?: 0L) > 0L) {
                settings!!.savedAtUnixSeconds
            } else {
                System.currentTimeMillis() / 1000
            }

        return BrowserStorageSyncProviderSettings(
            version = CURRENT_VERSION,
            providerKind = providerKind,
            localFolderRootPath = normalizedRootPath,
            scope = scope,
            deviceId = deviceId,
            membershipJoinedAtUnixSeconds = normalizedMembership,
            savedAtUnixSeconds = savedAt,
        )
    }

    private fun normalizeProviderKind(providerKind: String?): String? {
        val normalized = providerKind?.trim().orEmpty()
        return KNOWN_PROVIDER_KINDS.firstOrNull { it == normalized }
    }

    private fun BrowserStorageSyncProviderSettings.toJson(): JSONObject =
        JSONObject()
            .put("version", version)
            .put("providerKind", providerKind ?: JSONObject.NULL)
            .put("localFolderRootPath", localFolderRootPath ?: JSONObject.NULL)
            .put("scope", scope)
            .put("deviceId", deviceId)
            .put("membershipJoinedAtUnixSeconds", membershipJoinedAtUnixSeconds ?: JSONObject.NULL)
            .put("savedAtUnixSeconds", savedAtUnixSeconds)

    private fun JSONObject.toSettingsOrNull(): BrowserStorageSyncProviderSettings =
        BrowserStorageSyncProviderSettings(
            version = optInt("version", CURRENT_VERSION),
            providerKind = optNullableString("providerKind"),
            localFolderRootPath = optNullableString("localFolderRootPath"),
            scope = optNullableString("scope").orEmpty(),
            deviceId = optNullableString("deviceId").orEmpty(),
            membershipJoinedAtUnixSeconds = optNullableLong("membershipJoinedAtUnixSeconds"),
            savedAtUnixSeconds = optLong("savedAtUnixSeconds", 0),
        )

    companion object {
        const val LOCAL_FOLDER_PROVIDER_KIND = "localFolder"

        private const val TAG = "BrowserSyncProviderStore"
        private const val FILE_NAME = "browser-sync-provider.json"
        private const val CURRENT_VERSION = 1
        private const val DEFAULT_SCOPE = "default"

        // The four roadmap providers of the cloud-drive architecture. Every
        // other token is contract drift and rejects to "no provider configured"
        // instead of flowing an unknown kind into the Rust connection contract.
        private val KNOWN_PROVIDER_KINDS = arrayOf(
            LOCAL_FOLDER_PROVIDER_KIND,
            "iCloudDrive",
            "googleDrive",
            "oneDrive",
        )
    }
}

private fun JSONObject.optNullableString(name: String): String? =
    if (has(name) && !isNull(name)) optString(name) else null

private fun JSONObject.optNullableLong(name: String): Long? =
    if (has(name) && !isNull(name)) optLong(name) else null
