plugins { id("com.android.library") }
// An app's build gives the reader a build directory of its own (`-PcanvasBuildDir`, agent-android.mjs).
providers.gradleProperty("canvasBuildDir").orNull?.let { layout.buildDirectory = file(it) }
android {
    namespace = "dev.exact.canvas"
    compileSdk { version = release(37) { minorApiLevel = 2 } }
    defaultConfig { minSdk = 29; consumerProguardFiles("proguard-rules.pro") }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    // A package's unit tests compile its Kotlin into this module's tests; the reader links none of it.
    sourceSets {
        getByName("test").kotlin.directories.addAll(listOf(
            "../../../packages/age-range/android", "../../../packages/age-range/test",
            "../../../packages/barcode-detection/android", "../../../packages/barcode-detection/test",
            "../../../packages/alarms/android", "../../../packages/alarms/test",
            "../../../packages/background/android", "../../../packages/background/test",
            "../../../packages/calendar/android", "../../../packages/calendar/test",
            "../../../packages/camera/android", "../../../packages/camera/test",
            "../../../packages/contacts/android", "../../../packages/contacts/test",
            "../../../packages/text-detection/android", "../../../packages/text-detection/test",
            "../../../packages/media-capture/android", "../../../packages/media-capture/test",
        ))
    }
}

dependencies {
    testImplementation("com.google.android.play:age-signals:0.0.4")
    testImplementation("com.google.mlkit:text-recognition:16.0.1")
    testImplementation("com.google.mlkit:barcode-scanning:17.3.0")
    testImplementation("junit:junit:4.13.2")
    testImplementation(libs.robolectric)
    testImplementation("androidx.camera:camera-camera2:1.6.2")
    testImplementation("androidx.camera:camera-lifecycle:1.6.2")
    testImplementation("androidx.camera:camera-view:1.6.2")
    testImplementation("androidx.lifecycle:lifecycle-runtime:2.9.4")
}
