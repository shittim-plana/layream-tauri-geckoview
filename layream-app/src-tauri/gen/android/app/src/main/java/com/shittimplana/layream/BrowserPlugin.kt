package com.shittimplana.layream

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONArray
import org.json.JSONObject

@TauriPlugin
class BrowserPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun listBrowsers(invoke: Invoke) {
        try {
            val probe = Intent(Intent.ACTION_VIEW, Uri.parse("https://example.com"))
            probe.addCategory(Intent.CATEGORY_BROWSABLE)
            val apps = activity.packageManager
                .queryIntentActivities(probe, PackageManager.MATCH_ALL)
                .filter { it.activityInfo.packageName != activity.packageName }

            val arr = JSONArray()
            for (ri in apps) {
                val obj = JSONObject()
                obj.put("package", ri.activityInfo.packageName)
                obj.put("label", ri.loadLabel(activity.packageManager).toString())
                arr.put(obj)
            }
            val result = JSObject()
            result.put("browsers", arr)
            invoke.resolve(result)
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }

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

    @Command
    fun openInBrowser(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(JSObject::class.java)
            val url = args.getString("url")
            val pkg = args.getString("package")
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
            intent.addCategory(Intent.CATEGORY_BROWSABLE)
            if (pkg != null && pkg.isNotEmpty()) {
                intent.setPackage(pkg)
            }
            activity.startActivity(intent)
            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }
}
