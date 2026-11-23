buildscript {
    // The Kotlin host/android compiles the Exact reader with.
    dependencies {
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.10")
        classpath("org.jetbrains.kotlin:compose-compiler-gradle-plugin:2.4.10")
    }
}
plugins {
    id("com.android.application") version "9.4.1" apply false
    id("com.android.library") version "9.4.1" apply false
}
