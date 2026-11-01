package com.phonebridge.companion.pairing

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKeys
import com.phonebridge.companion.model.PairedDevice
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Encrypted storage for paired device records.
 * Uses AndroidX Security's EncryptedSharedPreferences backed by
 * Android Keystore for key management.
 */
class DeviceKeyStore(context: Context) {

    private val json = Json { ignoreUnknownKeys = true }

    private val prefs: SharedPreferences = EncryptedSharedPreferences.create(
        "phonebridge_paired_devices",
        MasterKeys.getOrCreate(MasterKeys.AES256_GCM_SPEC),
        context,
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
    )

    fun savePairedDevice(device: PairedDevice) {
        prefs.edit()
            .putString("device:${device.id}", json.encodeToString(device))
            .apply()
    }

    fun getPairedDevices(): List<PairedDevice> {
        return prefs.all
            .filter { it.key.startsWith("device:") }
            .mapNotNull { (_, value) ->
                runCatching {
                    json.decodeFromString<PairedDevice>(value as String)
                }.getOrNull()
            }
            .sortedByDescending { it.lastSeen }
    }

    fun removePairedDevice(id: String) {
        prefs.edit().remove("device:$id").apply()
    }

    fun isPaired(id: String): Boolean {
        return prefs.contains("device:$id")
    }

    fun updateLastSeen(id: String) {
        val device = getPairedDevices().find { it.id == id } ?: return
        savePairedDevice(device.copy(lastSeen = System.currentTimeMillis()))
    }
}
