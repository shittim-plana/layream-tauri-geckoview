package com.shittimplana.layream

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@TauriPlugin
class StreamingServicePlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val NOTIFICATION_PERMISSION_CODE = 1001
    }

    private fun ensureNotificationPermission(): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED
            ) {
                ActivityCompat.requestPermissions(
                    activity,
                    arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                    NOTIFICATION_PERMISSION_CODE
                )
                return false
            }
        }
        return true
    }

    @Command
    fun startStreaming(invoke: Invoke) {
        try {
            ensureNotificationPermission()

            val text = runCatching { invoke.parseArgs(String::class.java) }.getOrNull()
                ?: ApiStreamingService.DEFAULT_TEXT
            val intent = Intent(activity, ApiStreamingService::class.java).apply {
                action = ApiStreamingService.ACTION_START
                putExtra(ApiStreamingService.EXTRA_TEXT, text)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                ContextCompat.startForegroundService(activity, intent)
            } else {
                activity.startService(intent)
            }
            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }

    @Command
    fun stopStreaming(invoke: Invoke) {
        try {
            val intent = Intent(activity, ApiStreamingService::class.java).apply {
                action = ApiStreamingService.ACTION_STOP
            }
            activity.startService(intent)
            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }

    @Command
    fun updateNotification(invoke: Invoke) {
        try {
            val text = runCatching { invoke.parseArgs(String::class.java) }.getOrNull()
                ?: ApiStreamingService.DEFAULT_TEXT
            val intent = Intent(activity, ApiStreamingService::class.java).apply {
                action = ApiStreamingService.ACTION_UPDATE
                putExtra(ApiStreamingService.EXTRA_TEXT, text)
            }
            activity.startService(intent)
            invoke.resolve()
        } catch (ex: Exception) {
            invoke.reject(ex.message)
        }
    }
}
