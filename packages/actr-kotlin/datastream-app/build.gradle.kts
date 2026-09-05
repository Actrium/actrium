plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("com.google.protobuf")
}

android {
    namespace = "io.actrium.datastream"
    compileSdk = 34

    defaultConfig {
        applicationId = "io.actrium.datastream"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "1.0.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions { jvmTarget = "17" }

    buildFeatures {
        compose = true
    }

    composeOptions { kotlinCompilerExtensionVersion = "1.5.8" }
}

// Protobuf message generation for the local ProbeService + remote
// DuplexStreamService. Wire-identical to the iOS DataStreamApp protos
// (`package local;` is preserved so the actr route keys —
// "local.ProbeService.StartProbe", "local.DuplexStreamService.StartDuplexStream"
// etc. — derive identically). java_multiple_files yields clean top-level
// Kotlin/Java classes.
protobuf {
    protoc { artifact = "com.google.protobuf:protoc:3.25.1" }
    generateProtoTasks {
        all().forEach { task -> task.builtins { create("java") { option("lite") } } }
    }
}

dependencies {
    // actr-kotlin library (transitively provides libactr.so + libopus.so via
    // jniLibs/arm64-v8a, plus the Kotlin actr SDK bindings).
    implementation(project(":actr-kotlin"))

    // Protobuf runtime
    implementation("com.google.protobuf:protobuf-javalite:3.25.1")

    // Android core
    implementation("androidx.core:core-ktx:1.12.0")

    // Compose (the UI is a 1:1 port of ContentView.swift → Jetpack Compose)
    implementation(platform("androidx.compose:compose-bom:2024.01.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    // material3 pinned to 1.2.0: the compose-bom:2024.01.00 resolves
    // material3 to 1.1.2 (compiled against animation-core 1.4.x) while the BOM
    // forces animation-core to 1.6.0, where KeyframesSpecConfig.at() changed —
    // CircularProgressIndicator's keyframes{} then throws NoSuchMethodError at
    // runtime. 1.2.0 was released alongside animation 1.6.0, so the method
    // signatures match.
    implementation("androidx.compose.material3:material3:1.2.0")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.8.2")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.7.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.7.0")

    // Coroutines
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.0")

    // Testing
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")

    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
