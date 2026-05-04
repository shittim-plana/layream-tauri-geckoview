package com.shittimplana.layream

import android.app.Activity
import android.content.Intent
import android.net.Uri
import androidx.browser.customtabs.CustomTabColorSchemeParams
import androidx.browser.customtabs.CustomTabsIntent
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@TauriPlugin
class BrowserPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun openBrowser(invoke: Invoke) {
        try {
            val url = invoke.parseArgs(String::class.java)
            val uri = Uri.parse(url)

            try {
                val colorScheme = CustomTabColorSchemeParams.Builder()
                    .setToolbarColor(0xFF0F0F14.toInt())
                    .build()
                val customTabsIntent = CustomTabsIntent.Builder()
                    .setShowTitle(true)
                    .setDefaultColorSchemeParams(colorScheme)
                    .build()
                customTabsIntent.launchUrl(activity, uri)
            } catch (_: Exception) {
                val intent = Intent(Intent.ACTION_VIEW, uri)
                val chooser = Intent.createChooser(intent, "브라우저 선택")
                chooser.setFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.startActivity(chooser)
            }

            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }
}
