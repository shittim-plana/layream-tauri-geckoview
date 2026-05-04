package com.shittimplana.layream

import android.app.Activity
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
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
            val uri = Uri.parse(url)

            val baseIntent = Intent(Intent.ACTION_VIEW, uri)
            baseIntent.addCategory(Intent.CATEGORY_BROWSABLE)

            val browsers = activity.packageManager
                .queryIntentActivities(baseIntent, PackageManager.MATCH_DEFAULT_ONLY)
                .filter { it.activityInfo.packageName != activity.packageName }

            if (browsers.isEmpty()) {
                baseIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.startActivity(baseIntent)
            } else {
                val first = browsers[0]
                val targetIntent = Intent(Intent.ACTION_VIEW, uri)
                targetIntent.addCategory(Intent.CATEGORY_BROWSABLE)
                targetIntent.component = ComponentName(
                    first.activityInfo.packageName,
                    first.activityInfo.name
                )

                val extraIntents = browsers.drop(1).map { ri ->
                    Intent(Intent.ACTION_VIEW, uri).apply {
                        addCategory(Intent.CATEGORY_BROWSABLE)
                        component = ComponentName(ri.activityInfo.packageName, ri.activityInfo.name)
                    }
                }.toTypedArray()

                val chooser = Intent.createChooser(targetIntent, "브라우저 선택")
                if (extraIntents.isNotEmpty()) {
                    chooser.putExtra(Intent.EXTRA_INITIAL_INTENTS, extraIntents)
                }
                chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.startActivity(chooser)
            }

            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }
}
