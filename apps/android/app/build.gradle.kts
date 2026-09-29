import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

/** The version lives in one place: the Rust workspace, shared with the desktop app. */
val workspaceVersion: String = rootProject.projectDir.parentFile.parentFile.resolve("Cargo.toml")
    .readLines()
    .firstNotNullOfOrNull { Regex("""^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"""").find(it) }
    ?.let { "${it.groupValues[1]}.${it.groupValues[2]}.${it.groupValues[3]}" }
    ?: error("could not read the version from Cargo.toml")

val versionParts = workspaceVersion.split('.').map { it.toInt() }

/** versionCode = major*1000000 + minor*10000 + patch*100 + abi, so updates always move forward. */
val baseVersionCode = versionParts[0] * 1_000_000 + versionParts[1] * 10_000 + versionParts[2] * 100
val abiCodes = mapOf("armeabi-v7a" to 1, "arm64-v8a" to 2, "x86_64" to 3)
val universalAbiCode = 9

val abis: List<String> = providers.gradleProperty("hashlark.abis")
    .getOrElse("arm64-v8a,armeabi-v7a,x86_64")
    .split(',').map { it.trim() }.filter { it.isNotEmpty() }

// Release signing. CI provides the key through environment variables; a local
// release build reads keystore.properties (git-ignored). Without either, the
// release APK stays unsigned.
val keystoreProperties = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}

fun signingValue(env: String, property: String): String? =
    System.getenv(env)?.takeIf { it.isNotBlank() } ?: keystoreProperties.getProperty(property)

val releaseStoreFile = signingValue("ANDROID_KEYSTORE_FILE", "storeFile")
val hasReleaseSigning = releaseStoreFile != null
if (!hasReleaseSigning) {
    logger.warn("Release APKs will be UNSIGNED: set ANDROID_KEYSTORE_FILE or create keystore.properties (docs/releasing.md).")
}

android {
    namespace = "io.github.ashxtrem.hashlark"
    compileSdk = 36
    ndkVersion = "29.0.13599879"

    defaultConfig {
        applicationId = "io.github.ashxtrem.hashlark"
        minSdk = 26
        targetSdk = 36
        versionCode = baseVersionCode + universalAbiCode
        versionName = workspaceVersion
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = file(releaseStoreFile!!)
                storePassword = signingValue("ANDROID_KEYSTORE_PASSWORD", "storePassword")
                keyAlias = signingValue("ANDROID_KEY_ALIAS", "keyAlias")
                keyPassword = signingValue("ANDROID_KEY_PASSWORD", "keyPassword")
                // APK Signature Scheme v2 and v3; v1 (JAR signing) is not needed from API 24.
                enableV1Signing = false
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (hasReleaseSigning) signingConfig = signingConfigs.getByName("release")
        }
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
    }

    // One APK per ABI plus a universal one (docs/android-plan.md section 8).
    splits {
        abi {
            isEnable = true
            reset()
            include(*abis.toTypedArray())
            isUniversalApk = true
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        // Compressed in the APK (about half the download); Android extracts the library at install.
        jniLibs { useLegacyPackaging = true }
        resources { excludes += "/META-INF/{AL2.0,LGPL2.1}" }
    }

    lint {
        abortOnError = true
        checkReleaseBuilds = true
        baseline = file("lint-baseline.xml")
    }

    testOptions {
        unitTests {
            isIncludeAndroidResources = true
            isReturnDefaultValues = true
        }
    }
}

androidComponents {
    onVariants { variant ->
        // Give every ABI split its own, higher-than-before versionCode.
        variant.outputs.forEach { output ->
            val abi = output.filters.firstOrNull { it.filterType == com.android.build.api.variant.FilterConfiguration.FilterType.ABI }
            val code = abiCodes[abi?.identifier] ?: universalAbiCode
            output.versionCode.set(baseVersionCode + code)
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    implementation(project(":core"))

    implementation(platform(libs.compose.bom))
    androidTestImplementation(platform(libs.compose.bom))

    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material3.adaptive)
    implementation(libs.compose.material3.navigation.suite)
    implementation(libs.compose.material.icons.extended)
    implementation(libs.androidx.window)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.datastore.preferences)
    implementation(libs.androidx.work.runtime.ktx)
    implementation(libs.kotlinx.coroutines.android)

    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.window.testing)
    testImplementation(libs.compose.ui.test.junit4)

    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.window.testing)
}
