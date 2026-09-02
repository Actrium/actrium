package io.actrium.app.data.sync

import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKeys
import io.actrium.app.rust.ActriumCore
import java.util.UUID

class BrowserStorageSyncCredentialStore(
    context: Context,
    private val preferencesName: String = DEFAULT_PREFERENCES_NAME
) : BrowserStorageSyncCredentialsProvider {
    private val appContext = context.applicationContext

    /**
     * Encrypted preferences backing the credential store. Created lazily because
     * this class lives in an application-level singleton that may be constructed
     * on the main thread, and EncryptedSharedPreferences creation (Keystore
     * master-key access) is not cheap.
     *
     * There is deliberately no plaintext fallback: if encrypted storage cannot
     * be created, the failure is logged and propagated instead of silently
     * downgrading the access token to plaintext storage.
     */
    private val preferences: SharedPreferences by lazy {
        val encrypted = try {
            // AES256-GCM master key in the Android Keystore; key material never
            // leaves the Keystore.
            val masterKeyAlias = MasterKeys.getOrCreate(MasterKeys.AES256_GCM_SPEC)
            EncryptedSharedPreferences.create(
                encryptedPreferencesName(preferencesName),
                masterKeyAlias,
                appContext,
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )
        } catch (e: Exception) {
            Log.e(
                TAG,
                "Failed to create encrypted preferences '$preferencesName'; " +
                    "browser sync credentials are unavailable (no plaintext fallback)",
                e,
            )
            throw e
        }
        migrateLegacyPlaintextPreferences(encrypted)
        encrypted
    }

    /**
     * One-time migration from the legacy plaintext SharedPreferences file.
     * Copies any stored values into the encrypted store, then clears and
     * deletes the plaintext file so the access token no longer exists on disk
     * unencrypted. After the first successful migration the plaintext file is
     * gone and this becomes a no-op.
     */
    private fun migrateLegacyPlaintextPreferences(encrypted: SharedPreferences) {
        val legacy = appContext.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)
        val entries = legacy.all
        if (entries.isEmpty()) {
            return
        }

        Log.w(
            TAG,
            "Migrating ${entries.size} browser sync credential entries from " +
                "plaintext preferences '$preferencesName' to encrypted storage",
        )

        val editor = encrypted.edit()
        for (key in MIGRATED_KEYS) {
            val value = entries[key] as? String ?: continue
            editor.putString(key, value)
        }
        // commit() rather than apply(): the plaintext copy must not be deleted
        // before the encrypted copy is durably written.
        if (!editor.commit()) {
            Log.e(
                TAG,
                "Failed to persist migrated credentials to encrypted storage; " +
                    "keeping plaintext preferences '$preferencesName' for retry",
            )
            return
        }

        legacy.edit().clear().commit()
        // minSdk is 26, so deleteSharedPreferences (API 24+) is always available.
        appContext.deleteSharedPreferences(preferencesName)
        Log.i(TAG, "Deleted legacy plaintext preferences '$preferencesName' after migration")
    }

    fun saveSession(
        environment: String = ActriumCore.authEnvironmentProd(),
        accessToken: String,
        baseUrl: String = ActriumCore.authAccountBaseUrl(environment),
        scope: String = DEFAULT_SCOPE
    ) {
        preferences.edit()
            .putString(KEY_ENVIRONMENT, environment.trim())
            .putString(KEY_BASE_URL, baseUrl.trim())
            .putString(KEY_ACCESS_TOKEN, accessToken.trim())
            .putString(KEY_SCOPE, scope.trim().ifEmpty { DEFAULT_SCOPE })
            .putString(KEY_DEVICE_ID, getOrCreateDeviceId())
            .apply()
    }

    fun clearSession(environment: String? = null) {
        val storedEnvironment = preferences.getString(KEY_ENVIRONMENT, null)
        if (environment != null && storedEnvironment != null && storedEnvironment != environment) {
            return
        }

        preferences.edit()
            .remove(KEY_ENVIRONMENT)
            .remove(KEY_BASE_URL)
            .remove(KEY_ACCESS_TOKEN)
            .remove(KEY_SCOPE)
            .apply()
    }

    override suspend fun browserStorageSyncCredentials(): BrowserStorageSyncCredentials? {
        val environment = preferences.getString(KEY_ENVIRONMENT, null)
            ?.takeIf { it.isNotBlank() }
            ?: ActriumCore.authEnvironmentProd()
        val accessToken = preferences.getString(KEY_ACCESS_TOKEN, null)
            ?.trim()
            .orEmpty()
        val baseUrl = preferences.getString(KEY_BASE_URL, null)
            ?.takeIf { it.isNotBlank() }
            ?: ActriumCore.authAccountBaseUrl(environment)
        val scope = preferences.getString(KEY_SCOPE, null)
            ?.takeIf { it.isNotBlank() }
            ?: DEFAULT_SCOPE

        if (accessToken.isBlank()) {
            return null
        }

        return BrowserStorageSyncCredentials(
            baseUrl = baseUrl,
            accessToken = accessToken,
            scope = scope,
            deviceId = getOrCreateDeviceId(),
        )
    }

    /**
     * Public read accessor for the durable endpoint device identity. The
     * provider settings store is the device-identity owner now that the
     * endpoint dialog is dev-only; it adopts this value the first time it is
     * written so the same machine never appears as two cloud devices, and this
     * store keeps its own durable copy so a corrupt provider-settings file can
     * recover the same id instead of minting a ghost.
     */
    fun deviceId(): String = getOrCreateDeviceId()

    private fun getOrCreateDeviceId(): String {
        val existing = preferences.getString(KEY_DEVICE_ID, null)
        if (!existing.isNullOrBlank()) {
            return existing
        }

        val next = UUID.randomUUID().toString()
        preferences.edit().putString(KEY_DEVICE_ID, next).apply()
        return next
    }

    companion object {
        const val DEFAULT_SCOPE = "default"
        const val DEFAULT_PREFERENCES_NAME = "actrium_browser_sync_credentials"

        private const val TAG = "BrowserSyncCredStore"

        private const val KEY_ENVIRONMENT = "environment"
        private const val KEY_BASE_URL = "base_url"
        private const val KEY_ACCESS_TOKEN = "access_token"
        private const val KEY_SCOPE = "scope"
        private const val KEY_DEVICE_ID = "device_id"

        private val MIGRATED_KEYS = arrayOf(
            KEY_ENVIRONMENT,
            KEY_BASE_URL,
            KEY_ACCESS_TOKEN,
            KEY_SCOPE,
            KEY_DEVICE_ID,
        )

        /**
         * Encrypted data lives in a separate preferences file: reusing the
         * legacy file name would make EncryptedSharedPreferences read the old
         * plaintext entries as corrupt ciphertext.
         */
        private fun encryptedPreferencesName(preferencesName: String): String =
            "${preferencesName}_encrypted"
    }
}
