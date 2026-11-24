import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.nocturne.system"
    compileSdk = 37

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.16.0")
    implementation("androidx.work:work-runtime:2.10.1")
    // Google's one-time permission for the person's Drive, for the nightly backup.
    implementation("com.google.android.gms:play-services-auth:21.5.1")
    // The app has it through Tauri; only the activity result type is needed here.
    compileOnly("androidx.activity:activity:1.6.0")
    testImplementation("junit:junit:4.13.2")
    // Android's own JSON classes are stubs in JVM tests.
    testImplementation("org.json:json:20250517")
    implementation(project(":tauri-android"))
}
