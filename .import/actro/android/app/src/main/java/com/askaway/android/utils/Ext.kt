package com.askaway.android.utils

import android.os.Bundle


fun Bundle.addMap(params: Map<String, Any?>?) {
    params?.forEach { (key, value) ->
        when (value) {
            is Boolean -> putBoolean(key, value)
            is String -> putString(key, value)
            is Int -> putInt(key, value)
            is Double -> putDouble(key, value)
            is Float -> putFloat(key, value)
            is Long -> putLong(key, value)
            is Bundle -> putBundle(key, value)
        }
    }
}
