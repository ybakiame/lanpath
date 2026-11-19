import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
  id("com.android.library")
  id("org.jetbrains.kotlin.android")
}

android {
  namespace = "com.ybakiame.lanpath"
  compileSdk = 36
  defaultConfig {
    minSdk = 33
    consumerProguardFiles("consumer-rules.pro")
    ndk { abiFilters += "arm64-v8a" }
  }
  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_1_8
    targetCompatibility = JavaVersion.VERSION_1_8
  }
}

kotlin { compilerOptions { jvmTarget = JvmTarget.JVM_1_8 } }

// Compile against libbox; the consuming application must package this AAR too.
dependencies {
  compileOnly(files("libs/lanpathcore.aar"))
  implementation("androidx.core:core-ktx:1.13.1")
}
