package com.pvg.android

/**
 * Global configuration options for the PVG Android runtime.
 */
object Pvg {
    /**
     * Controls whether native ANativeWindow performance logs and kernel `/proc`
     * profiler reports (`tag: PVG_NATIVE`) are printed to Logcat.
     *
     * Defaults to `false` for clean production logging.
     *
     * Example:
     * ```kotlin
     * Pvg.isLoggingEnabled = BuildConfig.DEBUG
     * ```
     */
    var isLoggingEnabled: Boolean = false
        set(value) {
            field = value
            PvgEngine.setLoggingEnabled(value)
        }
}