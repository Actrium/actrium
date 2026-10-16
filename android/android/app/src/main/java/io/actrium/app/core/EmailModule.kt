package io.actrium.app.core

import android.content.Intent
import android.net.Uri
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableArray
import com.facebook.react.bridge.UiThreadUtil
import io.actrium.app.rust.ActriumCore

class EmailModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {
    private val contract = ActriumCore.emailBridgeContract()

    override fun getName(): String {
        return contract.moduleName
    }

    @ReactMethod
    fun composeEmail(to: ReadableArray, subject: String, promise: Promise) {
        val recipients = ArrayList<String>(to.size())
        for (index in 0 until to.size()) {
            val value = to.getString(index)
            if (!value.isNullOrBlank()) {
                recipients.add(value)
            }
        }

        if (recipients.isEmpty()) {
            promise.reject(
                contract.errorInvalidRecipients,
                "${contract.methodComposeEmail} requires at least one recipient",
            )
            return
        }

        val baseIntent = Intent(Intent.ACTION_SENDTO).apply {
            data = Uri.parse(contract.uriSchemeMailto)
            putExtra(Intent.EXTRA_EMAIL, recipients.toTypedArray())
            putExtra(Intent.EXTRA_SUBJECT, subject)
        }

        val chooserIntent = Intent.createChooser(baseIntent, null)
        val activity = currentActivity

        UiThreadUtil.runOnUiThread {
            try {
                if (activity != null) {
                    // Chinese Comment: The mail panel needs to be bound to the current front desk Activity,
                    // This will allow the system selectioner and target mail application to be opened in the current stack.
                    activity.startActivity(chooserIntent)
                } else {
                    // Comment in Chinese:React There is no front desk at this time. Activity The government has also been able to provide a new and more effective way of doing business.
                    // We have to fix it here. NEW_TASK, otherwise the system will directly reject the pull-up of the mail application.
                    chooserIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    reactApplicationContext.startActivity(chooserIntent)
                }
                promise.resolve(null)
            } catch (error: Exception) {
                promise.reject(contract.errorOpenFailed, error)
            }
        }
    }
}
