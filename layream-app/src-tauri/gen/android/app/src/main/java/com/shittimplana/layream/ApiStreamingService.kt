package com.shittimplana.layream

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

class ApiStreamingService : Service() {

    companion object {
        const val CHANNEL_ID = "api_streaming"
        const val NOTIFICATION_ID = 1
        const val ACTION_START = "com.shittimplana.layream.action.START_STREAMING"
        const val ACTION_STOP = "com.shittimplana.layream.action.STOP_STREAMING"
        const val ACTION_UPDATE = "com.shittimplana.layream.action.UPDATE_STREAMING"
        const val EXTRA_TEXT = "text"
        const val DEFAULT_TEXT = "AI 응답 수신 중..."
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = intent?.action
        when (action) {
            ACTION_STOP -> {
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_UPDATE -> {
                val text = intent.getStringExtra(EXTRA_TEXT) ?: DEFAULT_TEXT
                val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
                nm.notify(NOTIFICATION_ID, buildNotification(text))
                return START_STICKY
            }
            else -> {
                val text = intent?.getStringExtra(EXTRA_TEXT) ?: DEFAULT_TEXT
                val notification = buildNotification(text)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                    startForeground(
                        NOTIFICATION_ID,
                        notification,
                        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
                    )
                } else {
                    startForeground(NOTIFICATION_ID, notification)
                }
                return START_STICKY
            }
        }
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            if (nm.getNotificationChannel(CHANNEL_ID) == null) {
                val channel = NotificationChannel(
                    CHANNEL_ID,
                    "API Streaming",
                    NotificationManager.IMPORTANCE_LOW
                ).apply {
                    description = "AI 응답 스트리밍 동안 프로세스가 종료되지 않도록 유지합니다."
                    setShowBadge(false)
                }
                nm.createNotificationChannel(channel)
            }
        }
    }

    private fun buildNotification(text: String): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Layream")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
    }
}
