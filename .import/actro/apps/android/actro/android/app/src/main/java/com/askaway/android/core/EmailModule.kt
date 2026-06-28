package com.askaway.android.core

import android.content.Intent
import android.net.Uri
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableArray
import com.facebook.react.bridge.UiThreadUtil

class EmailModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {
    override fun getName(): String {
        return "EmailModule"
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
            promise.reject("EMAIL_INVALID_RECIPIENTS", "composeEmail requires at least one recipient")
            return
        }

        val baseIntent = Intent(Intent.ACTION_SENDTO).apply {
            data = Uri.parse("mailto:")
            putExtra(Intent.EXTRA_EMAIL, recipients.toTypedArray())
            putExtra(Intent.EXTRA_SUBJECT, subject)
        }

        val chooserIntent = Intent.createChooser(baseIntent, null)
        val activity = currentActivity

        UiThreadUtil.runOnUiThread {
            try {
                if (activity != null) {
                    // 中文注释：邮件面板需要绑定到当前前台 Activity，
                    // 这样系统选择器和目标邮件应用会按当前页面栈正常打开。
                    activity.startActivity(chooserIntent)
                } else {
                    // 中文注释：React 当前没有前台 Activity 时退回应用上下文启动，
                    // 这里必须补 NEW_TASK，否则系统会直接拒绝拉起邮件应用。
                    chooserIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    reactApplicationContext.startActivity(chooserIntent)
                }
                promise.resolve(null)
            } catch (error: Exception) {
                promise.reject("EMAIL_OPEN_FAILED", error)
            }
        }
    }
}
