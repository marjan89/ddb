plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

android {
    namespace = "dev.substrate.semantic"
    compileSdk = 35

    defaultConfig {
        minSdk = 24
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
    implementation("org.nanohttpd:nanohttpd:2.3.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    compileOnly("com.squareup.okhttp3:okhttp:4.12.0")
    compileOnly("com.squareup.retrofit2:retrofit:2.11.0")
    compileOnly("androidx.recyclerview:recyclerview:1.3.2")
}

afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("release") {
                from(components["release"])
                groupId = "dev.substrate"
                artifactId = "semantic-agent"
                version = providers.gradleProperty("semanticAgent.version")
                    .orElse(providers.environmentVariable("SEMANTIC_AGENT_VERSION"))
                    .getOrElse("0.6.0")
            }
        }
        repositories {
            // GitHub Packages — publish target. Credentials come from
            // the publishing workflow's GITHUB_TOKEN, or from a local
            // PAT in ~/.gradle/gradle.properties (gpr.user / gpr.key).
            maven {
                name = "GitHubPackages"
                url = uri("https://maven.pkg.github.com/marjan89/semantic-agent-android")
                credentials {
                    username = providers.gradleProperty("gpr.user").orNull
                        ?: System.getenv("GITHUB_ACTOR")
                    password = providers.gradleProperty("gpr.key").orNull
                        ?: System.getenv("GITHUB_TOKEN")
                }
            }
            // mavenLocal stays available for in-tree consumers via
            // `./gradlew publishToMavenLocal`.
            mavenLocal()
        }
    }
}
