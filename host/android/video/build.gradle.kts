// The `video` element's player (LLP 1042.001): Media3's ExoPlayer behind the reader's `VideoPlayer` seam. The reader
// names no Media3 type; an APK links this module only when its plan has a `video` or an `audio`, and in a
// development build of an app with no locked Maven graph (`-Pvideo=1`, `scripts/agent-android.mjs`).
plugins { id("com.android.library") }
providers.gradleProperty("videoBuildDir").orNull?.let { layout.buildDirectory = file(it) }
android {
    namespace = "dev.exact.video"
    compileSdk { version = release(37) { minorApiLevel = 2 } }
    defaultConfig { minSdk = 29 }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    testOptions { unitTests { isIncludeAndroidResources = true } }
}
dependencies {
    implementation(project(":canvas"))
    implementation(libs.media3.exoplayer)
    implementation(libs.media3.exoplayer.hls)
    implementation(libs.media3.ui)
    // Media3's modules work only at one version, and CameraX's recorder brings `media3-common` and `media3-muxer` at
    // its own: every `androidx.media3` module the APK resolves is this one.
    constraints {
        listOf("common", "container", "muxer", "datasource", "decoder", "extractor", "database", "exoplayer", "exoplayer-hls", "ui")
            .forEach { implementation("androidx.media3:media3-$it") { version { strictly(libs.versions.media3.get()) } } }
    }
    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
}
