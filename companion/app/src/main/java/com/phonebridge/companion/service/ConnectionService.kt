package com.phonebridge.companion.service

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import com.phonebridge.companion.R

/**
 * Foreground service that keeps the app alive while connected to a PC.
 *
 * Android kills background apps aggressively. Without a foreground service,
 * the pairing socket would be closed within minutes of the user switching
 * away from PhoneBridge.
 *
 * The service shows a persistent (non-dismissible) notification:
 *   "PhoneBridge — Connected to PC"
 *
 * It starts when pairing begins and stops when the connection ends.
 */
class ConnectionService : Service() {

    companion object {
        const val CHANNEL_ID = "phonebridge_connection"
        const val NOTIFICATION_ID = 1
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("PhoneBridge")
            .setContentText("Waiting for PC connection")
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth) // placeholder
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            // Android 14+ requires specifying the foreground service type
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }

        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    fun updateNotification(pcName: String) {
        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("PhoneBridge")
            .setContentText("Connected to $pcName")
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()

        val manager = getSystemService(android.app.NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, notification)
    }
}
