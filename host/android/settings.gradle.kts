pluginManagement {
    repositories { google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement {
    // CI init scripts may add project repositories. Keep this list authoritative (LLP 1030.003 D8).
    repositoriesMode.set(RepositoriesMode.PREFER_SETTINGS)
    repositories { google(); mavenCentral() }
}
rootProject.name = "exact-android"
// The reader (a library), and the shell that packages one app with it: `-Papp=<name>` names the app (app/build.gradle.kts).
include(":canvas")
// The `video` element's player, Media3's ExoPlayer: linked only into an APK whose plan has a `video` or an `audio`, and
// every development build (LLP 1042.001 D2).
include(":video")
// Home-screen widgets (LLP 1126.003): linked only into an APK whose app.json declares a widget Android draws.
include(":widgets")
// The Material reference of LLP 1115.001.001: Compose Material 3 drawn on the JVM, in no app.
include(":reference")
if (providers.gradleProperty("app").isPresent) include(":app")
