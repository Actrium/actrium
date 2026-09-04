// Mirror-first repositories: the self-hosted CI fleet has slow egress to
// dl.google.com / repo.maven.apache.org / plugins.gradle.org, which dominates
// the Kotlin job's dependency-resolution time. GitHub-hosted jobs opt into the
// canonical repositories so a transient mirror HTTP failure cannot abort their
// dependency resolution before Gradle reaches the fallback repositories.

pluginManagement {
    repositories {
        if (System.getenv("ACTR_GRADLE_REPOSITORIES") != "canonical") {
            maven("https://maven.aliyun.com/repository/gradle-plugin")
            maven("https://maven.aliyun.com/repository/google")
            maven("https://maven.aliyun.com/repository/central")
        }
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        if (System.getenv("ACTR_GRADLE_REPOSITORIES") != "canonical") {
            maven("https://maven.aliyun.com/repository/google")
            maven("https://maven.aliyun.com/repository/central")
        }
        google()
        mavenCentral()
    }
}

rootProject.name = "actr-kotlin"

include(":actr-kotlin")
if (file("datastream-app").isDirectory) { include(":datastream-app") }
if (file("demo").isDirectory) {
    include(":demo")
}
