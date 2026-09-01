// Mirror-first repositories: the self-hosted CI fleet has slow egress to
// dl.google.com / repo.maven.apache.org / plugins.gradle.org, which dominates
// the Kotlin job's dependency-resolution time. Aliyun mirrors are tried first;
// the canonical repos remain as fallback for anything the mirrors do not carry
// (order matters — Gradle queries them in declaration order).
pluginManagement {
    repositories {
        maven("https://maven.aliyun.com/repository/gradle-plugin")
        maven("https://maven.aliyun.com/repository/google")
        maven("https://maven.aliyun.com/repository/central")
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        maven("https://maven.aliyun.com/repository/google")
        maven("https://maven.aliyun.com/repository/central")
        google()
        mavenCentral()
    }
}

rootProject.name = "actr-kotlin"

include(":actr-kotlin")
if (file("demo").isDirectory) {
    include(":demo")
}
