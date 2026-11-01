plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "com.phonebridge.companion"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.phonebridge.companion"
        minSdk = 29        // Android 10 — hard floor (AudioPlaybackCapture)
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    // Compose BOM — pins all Compose library versions
    val composeBom = platform("androidx.compose:compose-bom:2025.05.00")
    implementation(composeBom)

    // Compose UI
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    debugImplementation("androidx.compose.ui:ui-tooling")

    // Activity + ViewModel
    implementation("androidx.activity:activity-compose:1.10.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.0")

    // Navigation
    implementation("androidx.navigation:navigation-compose:2.8.0")

    // Encrypted SharedPreferences (for device keys)
    implementation("androidx.security:security-crypto:1.1.0-alpha06")

    // Kotlin serialization (for pairing protocol JSON)
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.0")

    // Core KTX
    implementation("androidx.core:core-ktx:1.15.0")
}
