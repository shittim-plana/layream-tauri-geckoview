package com.shittimplana.layream

import android.app.Activity
import android.content.ComponentName
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
            val uri = Uri.parse(url)

            val baseIntent = Intent(Intent.ACTION_VIEW, uri)
            baseIntent.addCategory(Intent.CATEGORY_BROWSABLE)

            val allApps = activity.packageManager
                .queryIntentActivities(baseIntent, 0)
                .filter { it.activityInfo.packageName != activity.packageName }

            if (allApps.isEmpty()) {
                baseIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.startActivity(baseIntent)
            } else {
                val targetIntents = allApps.map { ri ->
                    Intent(Intent.ACTION_VIEW, uri).apply {
                        addCategory(Intent.CATEGORY_BROWSABLE)
                        component = ComponentName(ri.activityInfo.packageName, ri.activityInfo.name)
                    }
                }

                val chooser = Intent.createChooser(targetIntents.first(), "브라우저 선택")
                if (targetIntents.size > 1) {
                    chooser.putExtra(
                        Intent.EXTRA_INITIAL_INTENTS,
                        targetIntents.drop(1).toTypedArray()
                    )
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
