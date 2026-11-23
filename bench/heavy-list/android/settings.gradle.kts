// The heavy list (../SPEC.md) as two native Android apps: `views` (RecyclerView) and `compose` (LazyColumn), over
// `common` (the data, the image loader and live mode). Run with host/android's wrapper: `../../../host/android/gradlew -p .`
pluginManagement {
    repositories { google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement {
    repositories { google(); mavenCentral() }
}
rootProject.name = "heavybench-android"
include(":common", ":views", ":compose")
