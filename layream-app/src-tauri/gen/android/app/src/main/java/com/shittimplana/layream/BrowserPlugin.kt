package com.shittimplana.layream

import android.app.Activity
import android.content.Intent
import android.net.Uri
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
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
            intent.addCategory(Intent.CATEGORY_BROWSABLE)
            val chooser = Intent.createChooser(intent, "브라우저 선택")
            activity.startActivity(chooser)
            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }
}
