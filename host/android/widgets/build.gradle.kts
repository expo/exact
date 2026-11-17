// Home-screen widgets for an app's `widgets` rows (LLP 1126.003): the provider, the inflater that draws a widget
// entry's snapshot tree as RemoteViews, the timeline kept between refreshes, and the configuration activity. Framework
// only, so it adds no Maven dependency; an APK links it only when app.json declares a widget Android draws
// (`-Pwidgets=1`, `scripts/agent-android.mjs`), and the app's build generates the providers (app/build.gradle.kts).
plugins { id("com.android.library") }
providers.gradleProperty("widgetsBuildDir").orNull?.let { layout.buildDirectory = file(it) }
android {
    namespace = "dev.exact.widgets"
    compileSdk { version = release(37) { minorApiLevel = 2 } }
    defaultConfig { minSdk = 29; consumerProguardFiles("proguard-rules.pro") }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    testOptions { unitTests { isIncludeAndroidResources = true } }
}
dependencies {
    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
}
