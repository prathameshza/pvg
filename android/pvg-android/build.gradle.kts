plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    `maven-publish`
    signing
}

val isWindows = org.gradle.internal.os.OperatingSystem.current().isWindows

// Task: Automatically compile native Rust shared libraries (.so) before building AAR
val buildCargoNdk = tasks.register<Exec>("buildCargoNdk") {
    group = "build"
    description = "Compiles the Rust pvg_android crate for arm64-v8a and x86_64 using cargo-ndk"

    val projectRoot = rootDir.parentFile
    workingDir = projectRoot

    val outputJniDir = file("src/main/jniLibs").absolutePath

    inputs.dir(file("${projectRoot}/pvg_android/src"))
    inputs.dir(file("${projectRoot}/pvg/src"))
    outputs.dir(file("src/main/jniLibs"))

    val executableName = if (isWindows) "cmd" else "cargo"
    val cmdArgs = if (isWindows) {
        listOf(
            "/c",
            "cargo", "ndk",
            "-t", "arm64-v8a",
            "-t", "x86_64",
            "-o", outputJniDir,
            "build", "--release",
            "-p", "pvg_android"
        )
    } else {
        listOf(
            "ndk",
            "-t", "arm64-v8a",
            "-t", "x86_64",
            "-o", outputJniDir,
            "build", "--release",
            "-p", "pvg_android"
        )
    }

    commandLine(executableName, *cmdArgs.toTypedArray())
}

tasks.named("preBuild") {
    dependsOn(buildCargoNdk)
}

// Generate Javadoc JAR required by Maven Central quality validation
val javadocJar by tasks.registering(Jar::class) {
    archiveClassifier.set("javadoc")
    from(file("README.md"))
}

android {
    namespace = "com.pvg.android"
    compileSdk = 35

    defaultConfig {
        minSdk = 28

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        ndk {
            abiFilters.clear()
            abiFilters.addAll(listOf("arm64-v8a", "x86_64"))
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    buildFeatures {
        compose = true
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }
}

kotlin {
    jvmToolchain(11)
}

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.runtime)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
}

afterEvaluate {
    publishing {
        publications {
            register<MavenPublication>("release") {
                from(components["release"])
                artifact(javadocJar)

                groupId = "io.github.prathameshza"
                artifactId = "pvg"
                version = "0.1.0"

                pom {
                    name.set("PVG Android")
                    description.set("Deterministic Procedural Vector Graphics (PVG) Native Android Engine with 60 FPS Microsecond CPU Execution")
                    url.set("https://github.com/prathameshza/pvg")
                    inceptionYear.set("2025")

                    licenses {
                        license {
                            name.set("Apache-2.0")
                            url.set("https://www.apache.org/licenses/LICENSE-2.0.txt")
                        }
                    }

                    developers {
                        developer {
                            id.set("prathameshza")
                            name.set("Prathamesh")
                            url.set("https://github.com/prathameshza")
                        }
                    }

                    scm {
                        connection.set("scm:git:git://github.com/prathameshza/pvg.git")
                        developerConnection.set("scm:git:ssh://github.com:prathameshza/pvg.git")
                        url.set("https://github.com/prathameshza/pvg")
                    }
                }
            }
        }

        repositories {
            maven {
                name = "SonatypeBundle"
                url = uri(layout.buildDirectory.dir("repo"))
            }
        }
    }

    signing {
        useGpgCmd()
        sign(publishing.publications["release"])
    }
}