package com.phonebridge.companion.pairing

import android.content.Context
import android.net.LocalServerSocket
import android.net.LocalSocket
import android.os.Build
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.util.UUID

/**
 * PairingManager — handles the device pairing protocol.
 *
 * Protocol:
 *  1. This app opens a LocalServerSocket at "phonebridge-pair"
 *  2. The Windows app connects via ADB forward (tcp:27184 → localabstract:phonebridge-pair)
 *  3. Windows sends a JSON PairRequest (PC name + ID + public key)
 *  4. This app shows the request to the user
 *  5. If approved: sends PairResponse(accepted) + stores the paired device
 *  6. If denied: sends PairResponse(denied) + closes connection
 *
 * The socket name "phonebridge-pair" must match what the Rust backend uses in:
 *   adb forward tcp:27184 localabstract:phonebridge-pair
 */
class PairingManager(
    private val context: Context,
    private val keyStore: DeviceKeyStore,
) {
    companion object {
        private const val TAG = "PairingManager"
        private const val SOCKET_NAME = "phonebridge-pair"
    }

    // -------------------------------------------------------------------------
    // State
    // -------------------------------------------------------------------------

    sealed class PairingState {
        data object Idle : PairingState()
        data object Listening : PairingState()
        data class PendingApproval(
            val pcName: String,
            val pcId: String,
            val pcPublicKey: String,
        ) : PairingState()
        data class Paired(val device: com.phonebridge.companion.model.PairedDevice) : PairingState()
        data class Error(val message: String) : PairingState()
    }

    private val _state = MutableStateFlow<PairingState>(PairingState.Idle)
    val state: StateFlow<PairingState> = _state.asStateFlow()

    // Active connection (held between PendingApproval and approve/deny)
    private var serverSocket: LocalServerSocket? = null
    private var clientSocket: LocalSocket? = null
    private var pendingRequest: PairRequest? = null

    // -------------------------------------------------------------------------
    // Protocol messages
    // -------------------------------------------------------------------------

    @Serializable
    data class PairRequest(
        val name: String,       // PC display name (e.g. "MY-PC")
        val id: String,         // PC's unique device ID
        val publicKey: String,  // PC's public key (base64)
    )

    @Serializable
    data class PairResponse(
        val status: String,             // "accepted" or "denied"
        val name: String? = null,       // this phone's model name
        val id: String? = null,         // this phone's device ID
        val publicKey: String? = null,  // this phone's public key (base64)
    )

    private val json = Json { ignoreUnknownKeys = true }

    // -------------------------------------------------------------------------
    // Lifecycle
    // -------------------------------------------------------------------------

    /**
     * Start listening for pairing requests.
     * Call this when the user opens the app or when USB is connected.
     *
     * This suspends until a connection is received, so call from a coroutine.
     */
    suspend fun startListening() = withContext(Dispatchers.IO) {
        _state.value = PairingState.Listening
        Log.d(TAG, "Listening for pairing on localabstract:$SOCKET_NAME")

        try {
            // Close any existing server socket
            serverSocket?.close()

            // Open the local server socket
            // ADB forward maps a TCP port to this abstract socket name
            serverSocket = LocalServerSocket(SOCKET_NAME)

            // Accept one connection (blocks until PC connects)
            val client = serverSocket!!.accept()
            clientSocket = client

            Log.d(TAG, "PC connected, reading pairing request...")

            // Read the pairing request (one JSON line)
            val reader = BufferedReader(InputStreamReader(client.inputStream))
            val line = reader.readLine()
                ?: throw Exception("Empty pairing request — PC disconnected")

            val request = json.decodeFromString<PairRequest>(line)
            pendingRequest = request

            Log.d(TAG, "Pairing request from: ${request.name} (${request.id})")

            // Transition to PendingApproval — UI will show the approval dialog
            _state.value = PairingState.PendingApproval(
                pcName = request.name,
                pcId = request.id,
                pcPublicKey = request.publicKey,
            )

            // The user will call approvePairing() or denyPairing() from the UI.
            // We hold the socket open until then.

        } catch (e: Exception) {
            Log.e(TAG, "Pairing error: ${e.message}", e)
            if (_state.value !is PairingState.Paired) {
                _state.value = PairingState.Error(e.message ?: "Connection failed")
            }
            cleanup()
        }
    }

    /**
     * User approved the pairing. Send acceptance and store the device.
     */
    suspend fun approvePairing() = withContext(Dispatchers.IO) {
        val client = clientSocket ?: return@withContext
        val request = pendingRequest ?: return@withContext

        try {
            // Build the paired device record
            val device = com.phonebridge.companion.model.PairedDevice(
                id = request.id,
                name = request.name,
                publicKey = request.publicKey,
                pairedAt = System.currentTimeMillis(),
                lastSeen = System.currentTimeMillis(),
            )

            // Store it
            keyStore.savePairedDevice(device)

            // Send acceptance response
            val response = PairResponse(
                status = "accepted",
                name = Build.MODEL,
                id = getOrCreateDeviceId(),
                publicKey = getOrCreatePublicKey(),
            )

            val writer = BufferedWriter(OutputStreamWriter(client.outputStream))
            writer.write(json.encodeToString(response))
            writer.newLine()
            writer.flush()

            Log.d(TAG, "Pairing approved for ${request.name}")
            _state.value = PairingState.Paired(device)

        } catch (e: Exception) {
            Log.e(TAG, "Failed to send approval: ${e.message}", e)
            _state.value = PairingState.Error("Failed to complete pairing: ${e.message}")
        } finally {
            cleanup()
        }
    }

    /**
     * User denied the pairing.
     */
    suspend fun denyPairing() = withContext(Dispatchers.IO) {
        val client = clientSocket
        try {
            if (client != null) {
                val response = PairResponse(status = "denied")
                val writer = BufferedWriter(OutputStreamWriter(client.outputStream))
                writer.write(json.encodeToString(response))
                writer.newLine()
                writer.flush()
            }
        } catch (e: Exception) {
            Log.w(TAG, "Failed to send denial: ${e.message}")
        }

        Log.d(TAG, "Pairing denied")
        cleanup()
        _state.value = PairingState.Idle
    }

    /**
     * Stop listening and clean up all resources.
     */
    fun stop() {
        cleanup()
        _state.value = PairingState.Idle
    }

    private fun cleanup() {
        try { clientSocket?.close() } catch (_: Exception) {}
        try { serverSocket?.close() } catch (_: Exception) {}
        clientSocket = null
        serverSocket = null
        pendingRequest = null
    }

    // -------------------------------------------------------------------------
    // Device identity
    // -------------------------------------------------------------------------

    /**
     * Get or create a stable device ID.
     * NOT the Android ID — we generate our own UUID on first run.
     */
    private fun getOrCreateDeviceId(): String {
        val prefs = context.getSharedPreferences("phonebridge_identity", Context.MODE_PRIVATE)
        return prefs.getString("device_id", null) ?: run {
            val id = UUID.randomUUID().toString()
            prefs.edit().putString("device_id", id).apply()
            id
        }
    }

    /**
     * Get or create a public key for this device.
     * For Phase 1, we use a placeholder — real key exchange comes in Phase 5.
     * The important thing is that the pairing protocol shape is correct.
     */
    private fun getOrCreatePublicKey(): String {
        val prefs = context.getSharedPreferences("phonebridge_identity", Context.MODE_PRIVATE)
        return prefs.getString("public_key", null) ?: run {
            // Phase 1: generate a random placeholder.
            // Phase 5+: replace with actual Ed25519/X25519 key generation.
            val key = UUID.randomUUID().toString() + "-" + UUID.randomUUID().toString()
            prefs.edit().putString("public_key", key).apply()
            key
        }
    }
}
