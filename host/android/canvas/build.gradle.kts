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
        getByName("main").kotlin.directories.add("../../../vendor/ibex/crates/ibex2/android")
        getByName("test").kotlin.directories.addAll(listOf(
            "../../../packages/age-range/android", "../../../packages/age-range/test",
            "../../../packages/store-update/android", "../../../packages/store-update/test",
            "../../../packages/barcode-detection/android", "../../../packages/barcode-detection/test",
            "../../../packages/alarms/android", "../../../packages/alarms/test",
            "../../../packages/background/android", "../../../packages/background/test",
            "../../../packages/background-transfer/android", "../../../packages/background-transfer/test/android",
            "../../../packages/calendar/android", "../../../packages/calendar/test",
            "../../../packages/calls/android", "../../../packages/calls/test",
            "../../../packages/camera/android", "../../../packages/camera/test",
            "../../../packages/contacts/android", "../../../packages/contacts/test",
            "../../../packages/gamepad/android", "../../../packages/gamepad/test",
            "../../../packages/geofencing/android", "../../../packages/geofencing/test",
            "../../../packages/geocoding/android", "../../../packages/geocoding/test",
            "../../../packages/geolocation/android", "../../../packages/geolocation/test",
            "../../../packages/text-detection/android", "../../../packages/text-detection/test",
            "../../../packages/language-model/android", "../../../packages/language-model/test",
            "../../../packages/maps/android", "../../../packages/maps/test",
            "../../../packages/media-capture/android", "../../../packages/media-capture/test",
            "../../../packages/motion/android", "../../../packages/motion/test",
            "../../../packages/local-auth/android", "../../../packages/local-auth/test",
            "../../../packages/passkeys/android", "../../../packages/passkeys/test",
            "../../../packages/payments/android", "../../../packages/payments/test",
            "../../../packages/photos/android", "../../../packages/photos/test",
            "../../../packages/recording/android", "../../../packages/recording/test",
            "../../../packages/speech-recognition/android", "../../../packages/speech-recognition/test",
        ))
    }
    // Robolectric keeps a sandbox for each SDK level and shadow set the tests name, more than
    // Gradle's default 512 MB worker holds.
    testOptions { unitTests.all { it.maxHeapSize = "1g" } }
}

dependencies {
    testImplementation("com.google.android.play:age-signals:0.0.4")
    testImplementation("com.google.android.play:app-update:2.1.0")
    testImplementation("com.google.mlkit:text-recognition:16.0.1")
    testImplementation("com.google.mlkit:barcode-scanning:17.3.0")
    testImplementation("com.google.mlkit:genai-prompt:1.0.0-beta4")
    testImplementation("junit:junit:4.13.2")
    testImplementation("androidx.work:work-testing:2.12.0")
    testImplementation("androidx.room:room-runtime:2.7.0")
    testImplementation("com.google.android.gms:play-services-maps:20.1.0")
    testImplementation(libs.robolectric)
    testImplementation("androidx.camera:camera-camera2:1.6.2")
    testImplementation("androidx.camera:camera-lifecycle:1.6.2")
    testImplementation("androidx.camera:camera-view:1.6.2")
    testImplementation("androidx.lifecycle:lifecycle-runtime:2.9.4")
    testImplementation("androidx.credentials:credentials:1.6.0")
    testImplementation("com.google.android.gms:play-services-wallet:20.0.0")
    testImplementation("com.google.android.gms:play-services-location:21.4.0")
    testImplementation("androidx.core:core-telecom:1.0.1")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
}
