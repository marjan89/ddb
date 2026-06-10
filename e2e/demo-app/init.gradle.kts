// TD-93 fix: standalone init script (passed via --init-script in builds).
// Prior /.gradle/init.d/copy-apk.gradle.kts was NEVER loaded — /.gradle/
// is gradle's cache dir, not a config dir; init.d only auto-loads from
// ~/.gradle/init.d/ which we don't want to touch. Result: /tmp/*.apk
// went stale silently for an entire session 2026-06-09.
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
                    val alias = java.io.File("/tmp/regdemo-debug.apk")
                    apk.copyTo(alias, overwrite = true)
                    println("[copy-apk] alias -> ${alias.absolutePath}")
                }
            }
        }
    }
}
