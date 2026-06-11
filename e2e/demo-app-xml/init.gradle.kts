// Standalone init script (passed via --init-script in builds). Copies APKs
// out of app/build/outputs/apk/debug/ to /tmp/ so they survive the sandbox
// teardown (the build dir is wiped after the nosandbox process returns).
gradle.taskGraph.afterTask {
    val task = this
    if (task.name.startsWith("assemble") && task.name.endsWith("Debug")) {
        val outDir = task.project.file("${task.project.buildDir}/outputs/apk/debug")
        if (outDir.exists()) {
            outDir.listFiles { f -> f.extension == "apk" }?.forEach { apk ->
                val canonical = java.io.File("/tmp/${apk.name}")
                apk.copyTo(canonical, overwrite = true)
                println("[copy-apk] ${apk.name} -> ${canonical.absolutePath}")
                if (apk.name == "app-debug.apk") {
                    val alias = java.io.File("/tmp/regdemo-xml-debug.apk")
                    apk.copyTo(alias, overwrite = true)
                    println("[copy-apk] alias -> ${alias.absolutePath}")
                }
            }
        }
    }
}
