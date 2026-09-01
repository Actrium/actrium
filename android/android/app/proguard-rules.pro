# Add project specific ProGuard rules here.
# By default, the flags in this file are appended to flags specified
# in /usr/local/Cellar/android-sdk/24.3.3/tools/proguard/proguard-android.txt
# You can edit the include path and order by changing the proguardFiles
# directive in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# Add any project specific keep options here:

# React Native
-keep class com.facebook.react.** { *; }
-keep class com.facebook.jni.** { *; }
-keep class com.facebook.soloader.** { *; }
-keep interface com.facebook.react.** { *; }
-keep class * implements com.facebook.react.bridge.NativeModule

# JNI
-keepclasseswithmembernames class * {
    native <methods>;
}

# EventBus
-keepattributes *Annotation*
-keepclassmembers class * {
    @org.greenrobot.eventbus.Subscribe <methods>;
}
-keep enum org.greenrobot.eventbus.ThreadMode { *; }

# Protobuf
-keep class com.google.protobuf.** { *; }

# Coroutines
-keep class kotlinx.coroutines.** { *; }
-keepclassmembers class kotlinx.coroutines.** {
    volatile <fields>;
}

# Android LifeCycle
-keep class androidx.lifecycle.** { *; }

# Ignore Warning
-ignorewarnings
-dontwarn **

# Project Specific
-keep class io.actrium.actro.event.** { *; }
-keep class io.actrium.actro.model.** { *; }
-keep class io.actrium.actro.global.** { *; }

# WeChat
-keep class com.tencent.mm.opensdk.** { *; }
-keep class com.tencent.wxop.** { *; }
-keep class com.tencent.mm.sdk.** { *; }

# Coil
-keep class coil.** { *; }

# OkHttp
-keepattributes Signature
-keepattributes *Annotation*
-keep class okhttp3.** { *; }
-keep interface okhttp3.** { *; }
-dontwarn okhttp3.**

# ActrBridge & Other core classes potentially used via JNI or Reflection
-keep class io.actrium.actro.core.** { *; }

# JNA (Java Native Access)
-dontwarn com.sun.jna.**
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }

# Actr / Uniffi (Kotlin bindings using JNA)
-keep class io.actor_rtc.actr.** { *; }
-dontwarn io.actor_rtc.actr.**
