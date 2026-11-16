import javax.inject.Inject
import org.gradle.process.ExecOperations

plugins {
    alias(libs.plugins.android.application)
}

val rustAbi = "armeabi-v7a" // The Mive K-Suma is 32-bit only (zygote32).
val ndkVer = "30.0.16248370"

android {
    namespace = "eu.potoniec.mivefavouritelauncher"
    compileSdk = 37
    ndkVersion = ndkVer

    defaultConfig {
        applicationId = "eu.potoniec.mivefavouritelauncher"
        minSdk = 34
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += rustAbi }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

/** Builds the Rust workspace with cargo-ndk into a directory AGP packages as jniLibs. */
abstract class CargoNdkBuild @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:Internal
    abstract val workspace: DirectoryProperty

    @get:Input
    abstract val abi: Property<String>

    @get:Input
    abstract val ndkHome: Property<String>

    @get:OutputDirectory
    abstract val outDir: DirectoryProperty

    @TaskAction
    fun build() {
        val cargoHome = System.getenv("CARGO_HOME") ?: "${System.getProperty("user.home")}/.cargo"
        val out = outDir.get().asFile
        out.deleteRecursively()
        exec.exec {
            workingDir = workspace.get().asFile
            environment("ANDROID_NDK_HOME", ndkHome.get())
            environment("PATH", "$cargoHome/bin:${System.getenv("PATH")}")
            commandLine(
                "$cargoHome/bin/cargo", "ndk",
                "-t", abi.get(), "-P", "34", "-o", out.absolutePath,
                "build", "--release", "-p", "mivefav",
            )
        }
    }
}

val cargoBuild = tasks.register<CargoNdkBuild>("cargoNdkBuild") {
    val root = rootProject.layout.projectDirectory
    workspace.set(root)
    sources.from(
        root.file("Cargo.toml"),
        root.file("Cargo.lock"),
        root.dir("rust").asFileTree.matching { exclude("**/target/**") },
    )
    abi.set(rustAbi)
    ndkHome.set(androidComponents.sdkComponents.sdkDirectory.map { it.dir("ndk/$ndkVer").asFile.absolutePath })
    outDir.set(layout.buildDirectory.dir("rustJniLibs"))
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoBuild, CargoNdkBuild::outDir)
    }
}
