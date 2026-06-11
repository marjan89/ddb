pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "regression-demo-xml"
include(":app")

includeBuild("../../agent") {
    dependencySubstitution {
        substitute(module("dev.substrate:semantic-agent"))
            .using(project(":agent"))
    }
}
