package com.phonebridge.companion

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager

class PhoneBridgeApp : Application() {
    override fun onCreate() {
        super.onCreate()

        // Create notification channel for the foreground service
        val channel = NotificationChannel(
            "phonebridge_connection",
            "PhoneBridge connection",
            NotificationManager.IMPORTANCE_LOW, // No sound — persistent status only
        ).apply {
            description = "Shows when your phone is connected to a PC"
            setShowBadge(false)
        }

        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(channel)
    }
}
