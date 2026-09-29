import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.ListProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations
import java.io.File
import java.util.Properties
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.library)
}

val ndkVer = "29.0.13599879"
val workspaceRoot: File = rootProject.projectDir.parentFile.parentFile
val cargoAbis: List<String> = providers.gradleProperty("hashlark.abis")
    .getOrElse("arm64-v8a,armeabi-v7a,x86_64")
    .split(',').map { it.trim() }.filter { it.isNotEmpty() }

fun androidSdkDir(): File? {
    val local = rootProject.file("local.properties")
    if (local.exists()) {
        val sdk = Properties().apply { local.inputStream().use { load(it) } }.getProperty("sdk.dir")
        if (sdk != null) return File(sdk)
    }
    return (System.getenv("ANDROID_HOME") ?: System.getenv("ANDROID_SDK_ROOT"))?.let(::File)
}

val resolvedNdkHome: File? = System.getenv("ANDROID_NDK_HOME")?.let(::File)
    ?: androidSdkDir()?.resolve("ndk/$ndkVer")

/** Source files of the Rust workspace; a change rebuilds the native library. */
val rustSources = fileTree(workspaceRoot) {
    include("Cargo.toml", "Cargo.lock", "crates/**/*.rs", "crates/**/Cargo.toml", "crates/**/*.sql", "crates/**/uniffi.toml")
    include("definitions/builtin/**")
    exclude("**/target/**", "apps/**")
}

/** `cargo ndk build`: the Rust core as one `.so` per ABI, laid out as `jniLibs`. */
abstract class CargoNdkTask : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val minSdk: Property<Int>
    @get:Internal abstract val repoRoot: Property<File>
    @get:Internal abstract val ndkHome: Property<File>
    @get:InputFiles @get:PathSensitive(PathSensitivity.RELATIVE) abstract val sources: org.gradle.api.file.ConfigurableFileCollection
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun build() {
        // Only the ABIs asked for: a leftover library of another ABI would feed stale
        // metadata to the bindings generator and end up in the APK.
        outputDir.get().asFile.deleteRecursively()
        val args = mutableListOf("cargo", "ndk")
        abis.get().forEach { args += listOf("-t", it) }
        args += listOf("-P", minSdk.get().toString(), "-o", outputDir.get().asFile.absolutePath,
            "build", "-p", "hashlark-ffi", "--profile", "android", "--locked")
        exec.exec {
            workingDir = repoRoot.get()
            commandLine(args)
            ndkHome.orNull?.let { environment("ANDROID_NDK_HOME", it.absolutePath) }
        }
    }
}

/** Generates the Kotlin bindings from the metadata inside a built library. */
abstract class UniffiBindgenTask : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations
    @get:Internal abstract val repoRoot: Property<File>
    @get:InputFiles @get:PathSensitive(PathSensitivity.RELATIVE) abstract val libraries: org.gradle.api.file.ConfigurableFileCollection
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun generate() {
        val library = libraries.files.filter { it.name.endsWith(".so") }.minByOrNull { it.path }
            ?: error("no native library was built")
        exec.exec {
            workingDir = repoRoot.get()
            commandLine(
                "cargo", "run", "-q", "--locked", "-p", "uniffi-bindgen", "--",
                "generate", "--library", library.absolutePath,
                "--language", "kotlin",
                "--out-dir", outputDir.get().asFile.absolutePath,
                "--no-format",
            )
        }
    }
}

val cargoNdk = tasks.register<CargoNdkTask>("cargoNdk") {
    group = "build"
    description = "Builds the Rust core for Android with cargo-ndk."
    abis.set(cargoAbis)
    minSdk.set(26)
    repoRoot.set(workspaceRoot)
    ndkHome.set(providers.provider { resolvedNdkHome })
    sources.from(rustSources)
    outputDir.set(layout.buildDirectory.dir("generated/jniLibs"))
}

val uniffiBindgen = tasks.register<UniffiBindgenTask>("uniffiBindgen") {
    group = "build"
    description = "Generates the Kotlin bindings of hashlark-ffi."
    repoRoot.set(workspaceRoot)
    libraries.from(cargoNdk.flatMap { it.outputDir }.map { dir -> dir.asFileTree.matching { include("**/*.so") } })
    outputDir.set(layout.buildDirectory.dir("generated/uniffi/kotlin"))
}

android {
    namespace = "io.github.ashxtrem.hashlark.ffi"
    compileSdk = 36
    ndkVersion = ndkVer

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            // Keep the library page-aligned and uncompressed in the APK so it
            // loads with 16 KB pages.
            useLegacyPackaging = false
        }
    }
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoNdkTask::outputDir)
        variant.sources.kotlin?.addGeneratedSourceDirectory(uniffiBindgen, UniffiBindgenTask::outputDir)
    }
}

dependencies {
    // JNA loads the library for the generated bindings.
    api("${libs.jna.get()}@aar")
    implementation(libs.kotlinx.coroutines.android)
    // The generated bindings use @RequiresApi.
    implementation(libs.androidx.annotation)
}
