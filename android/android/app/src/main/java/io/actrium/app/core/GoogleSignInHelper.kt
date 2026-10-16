package io.actrium.app.core

import android.app.Activity
import android.util.Log
import androidx.credentials.CredentialManager
import androidx.credentials.GetCredentialRequest
import androidx.credentials.GetCredentialResponse
import androidx.credentials.exceptions.GetCredentialException
import io.actrium.app.global.Constants
import com.google.android.libraries.identity.googleid.GetGoogleIdOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch

/**
 * Google Login Support Class, Match WxApiHelper It's... SharedFlow Mode.
 * Use Credential Manager API(Replacement of abandoned GoogleSignInClient).
 */
class GoogleSignInHelper private constructor() {

    companion object {
        private const val TAG = "GoogleSignInHelper"

        @Volatile
        private var INSTANCE: GoogleSignInHelper? = null

        fun getInstance(): GoogleSignInHelper {
            return INSTANCE ?: synchronized(this) {
                INSTANCE ?: GoogleSignInHelper().also { INSTANCE = it }
            }
        }
    }

    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())

    // with WxApiHelper It's consistent. SharedFlow Mode
    private val _idToken = MutableSharedFlow<String?>(replay = 0)
    val idToken = _idToken.asSharedFlow()

    private val _authErr = MutableSharedFlow<String?>(replay = 0)
    val authErr = _authErr.asSharedFlow()

    /**
     * Launch Google Login, needs to upload to the current Activity(Credential Manager Yes. Activity Context)
     */
    fun signIn(activity: Activity?) {
        if (activity == null) {
            Log.e(TAG, "Activity is null, cannot start Google Sign-In")
            scope.launch { _authErr.emit("Activity is null") }
            return
        }

        val credentialManager = CredentialManager.create(activity)

        val googleIdOption = GetGoogleIdOption.Builder()
            .setFilterByAuthorizedAccounts(false)
            .setServerClientId(Constants.GOOGLE_WEB_CLIENT_ID)
            .build()

        val request = GetCredentialRequest.Builder()
            .addCredentialOption(googleIdOption)
            .build()

        scope.launch {
            try {
                val result: GetCredentialResponse = credentialManager.getCredential(activity, request)
                handleSignInResult(result)
            } catch (e: GetCredentialException) {
                Log.e(TAG, "Google Sign-In failed: ${e.message}")
                _authErr.emit(e.message ?: "Google Sign-In failed")
            }
        }
    }

    private suspend fun handleSignInResult(result: GetCredentialResponse) {
        val credential = result.credential
        try {
            val googleIdTokenCredential = GoogleIdTokenCredential.createFrom(credential.data)
            val token = googleIdTokenCredential.idToken
            Log.d(TAG, "Google Sign-In success, idToken obtained")
            _idToken.emit(token)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to parse Google credential: ${e.message}")
            _authErr.emit(e.message ?: "Failed to parse credential")
        }
    }
}
