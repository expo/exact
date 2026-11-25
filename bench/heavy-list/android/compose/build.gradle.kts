plugins { id("com.android.application"); id("org.jetbrains.kotlin.plugin.compose") }
android {
    namespace = "dev.exact.heavybench.compose"
    compileSdk { version = release(37) { minorApiLevel = 2 } }
    defaultConfig { applicationId = "dev.exact.heavybench.compose"; minSdk = 31; targetSdk = 36; versionCode = 1; versionName = "1" }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    buildFeatures { compose = true }
}
dependencies {
    implementation(project(":common"))
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.13.0")
}
