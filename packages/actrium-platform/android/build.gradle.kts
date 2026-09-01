plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

group =
    providers
        .gradleProperty("actroNativeGroup")
        .orElse("io.actrium")
        .get()

version =
    providers
        .gradleProperty("actroNativeVersion")
        .orElse("0.0.0-dev")
        .get()

android {
    namespace = "io.actrium.actro.rust"
    compileSdk = rootProject.extra["compileSdkVersion"] as Int

    defaultConfig {
        minSdk = rootProject.extra["minSdkVersion"] as Int
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    lint {
        abortOnError = false
        disable += listOf("NewApi")
    }
}

dependencies {
    implementation("org.jetbrains.kotlin:kotlin-stdlib")
    implementation("net.java.dev.jna:jna:5.14.0@aar")
}

val generateEmptyAnnotationTypedefs by tasks.registering {
    val typedefs =
        listOf("debug", "release").map { variant ->
            layout.buildDirectory.file("intermediates/annotations_typedef_file/$variant/extract${variant.replaceFirstChar { it.uppercase() }}Annotations/typedefs.txt")
        }

    outputs.files(typedefs)

    doLast {
        typedefs.forEach { fileProvider ->
            val file = fileProvider.get().asFile
            file.parentFile.mkdirs()
            file.writeText("")
        }
    }
}

tasks.configureEach {
    if (name.startsWith("extract") && name.endsWith("Annotations")) {
        enabled = false
    }
    if (name.startsWith("sync") && name.endsWith("LibJars")) {
        dependsOn(generateEmptyAnnotationTypedefs)
    }
}
