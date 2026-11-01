package com.phonebridge.companion

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.phonebridge.companion.model.PairedDevice
import com.phonebridge.companion.pairing.DeviceKeyStore
import com.phonebridge.companion.pairing.PairingManager
import com.phonebridge.companion.service.ConnectionService
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    private lateinit var pairingManager: PairingManager
    private lateinit var keyStore: DeviceKeyStore

    // Notification permission launcher (Android 13+)
    private val notificationPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { /* Result handled in the compose state */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        keyStore = DeviceKeyStore(this)
        pairingManager = PairingManager(this, keyStore)

        setContent {
            MaterialTheme {
                PhoneBridgeApp(
                    pairingManager = pairingManager,
                    keyStore = keyStore,
                    onRequestNotificationPermission = {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                            notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                        }
                    },
                    onStartService = {
                        val intent = Intent(this, ConnectionService::class.java)
                        startForegroundService(intent)
                    },
                )
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        pairingManager.stop()
    }
}

// ---------------------------------------------------------------------------
// Root composable
// ---------------------------------------------------------------------------

@Composable
fun PhoneBridgeApp(
    pairingManager: PairingManager,
    keyStore: DeviceKeyStore,
    onRequestNotificationPermission: () -> Unit,
    onStartService: () -> Unit,
) {
    val pairingState by pairingManager.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    val context = LocalContext.current

    // Check permissions
    val isNotificationGranted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        ContextCompat.checkSelfPermission(
            context, Manifest.permission.POST_NOTIFICATIONS
        ) == PackageManager.PERMISSION_GRANTED
    } else {
        true // Pre-Android 13: no runtime permission needed
    }

    // Check USB debugging (best-effort — can't detect reliably from app)
    val isUsbDebuggingEnabled = try {
        Settings.Global.getInt(context.contentResolver, Settings.Global.ADB_ENABLED, 0) == 1
    } catch (_: Exception) {
        false
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // App header
            Text(
                "PhoneBridge",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.Bold,
            )
            Text(
                "Connect your phone to your PC",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Spacer(Modifier.height(32.dp))

            when (val state = pairingState) {
                // ----- Idle: show setup checklist + start button -----
                is PairingManager.PairingState.Idle -> {
                    SetupChecklist(
                        isUsbDebuggingEnabled = isUsbDebuggingEnabled,
                        isNotificationGranted = isNotificationGranted,
                        onRequestNotificationPermission = onRequestNotificationPermission,
                    )

                    Spacer(Modifier.height(24.dp))

                    // Paired devices list
                    val pairedDevices = keyStore.getPairedDevices()
                    if (pairedDevices.isNotEmpty()) {
                        Text(
                            "Paired computers",
                            style = MaterialTheme.typography.titleSmall,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Spacer(Modifier.height(8.dp))
                        pairedDevices.forEach { device ->
                            PairedDeviceCard(
                                device = device,
                                onRemove = { keyStore.removePairedDevice(device.id) },
                            )
                            Spacer(Modifier.height(8.dp))
                        }
                        Spacer(Modifier.height(16.dp))
                    }

                    Button(
                        onClick = {
                            onStartService()
                            scope.launch { pairingManager.startListening() }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("Wait for PC connection")
                    }
                }

                // ----- Listening: waiting for PC to connect -----
                is PairingManager.PairingState.Listening -> {
                    Spacer(Modifier.height(48.dp))
                    CircularProgressIndicator()
                    Spacer(Modifier.height(16.dp))
                    Text("Waiting for PC connection...")
                    Text(
                        "Make sure USB is connected and PhoneBridge is open on your PC",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.height(24.dp))
                    OutlinedButton(onClick = { pairingManager.stop() }) {
                        Text("Cancel")
                    }
                }

                // ----- Pending approval: PC wants to pair -----
                is PairingManager.PairingState.PendingApproval -> {
                    Spacer(Modifier.height(32.dp))

                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.primaryContainer,
                        ),
                    ) {
                        Column(
                            modifier = Modifier.padding(24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Text(
                                "Connection request",
                                style = MaterialTheme.typography.titleMedium,
                            )
                            Spacer(Modifier.height(12.dp))
                            Text(
                                state.pcName,
                                style = MaterialTheme.typography.headlineSmall,
                                fontWeight = FontWeight.Bold,
                            )
                            Text(
                                "wants to connect to your phone",
                                style = MaterialTheme.typography.bodyMedium,
                            )
                            Spacer(Modifier.height(24.dp))
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                OutlinedButton(
                                    onClick = { scope.launch { pairingManager.denyPairing() } },
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text("Deny")
                                }
                                Button(
                                    onClick = { scope.launch { pairingManager.approvePairing() } },
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text("Allow")
                                }
                            }
                        }
                    }
                }

                // ----- Paired: success -----
                is PairingManager.PairingState.Paired -> {
                    Spacer(Modifier.height(48.dp))
                    Text("✓", style = MaterialTheme.typography.displayMedium,
                        color = MaterialTheme.colorScheme.primary)
                    Spacer(Modifier.height(16.dp))
                    Text(
                        "Connected to ${state.device.name}",
                        style = MaterialTheme.typography.titleLarge,
                    )
                    Text(
                        "Your phone is now paired. You can start mirroring from your PC.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.height(24.dp))
                    OutlinedButton(onClick = { pairingManager.stop() }) {
                        Text("Done")
                    }
                }

                // ----- Error -----
                is PairingManager.PairingState.Error -> {
                    Spacer(Modifier.height(48.dp))
                    Text(
                        "Connection failed",
                        style = MaterialTheme.typography.titleLarge,
                        color = MaterialTheme.colorScheme.error,
                    )
                    Spacer(Modifier.height(8.dp))
                    Text(
                        state.message,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.height(24.dp))
                    Button(onClick = {
                        scope.launch { pairingManager.startListening() }
                    }) {
                        Text("Try again")
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Setup checklist
// ---------------------------------------------------------------------------

@Composable
fun SetupChecklist(
    isUsbDebuggingEnabled: Boolean,
    isNotificationGranted: Boolean,
    onRequestNotificationPermission: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Text(
            "Setup",
            style = MaterialTheme.typography.titleSmall,
        )
        Spacer(Modifier.height(12.dp))

        // Step 1: USB Debugging
        ChecklistItem(
            step = 1,
            title = "USB debugging",
            description = if (isUsbDebuggingEnabled)
                "Enabled"
            else
                "Settings → About phone → tap Build Number 7 times → Developer Options → USB Debugging",
            isComplete = isUsbDebuggingEnabled,
        )

        // Step 2: Notifications (Android 13+)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            Spacer(Modifier.height(8.dp))
            ChecklistItem(
                step = 2,
                title = "Notifications",
                description = if (isNotificationGranted) "Granted" else "Required for connection status",
                isComplete = isNotificationGranted,
                actionLabel = if (!isNotificationGranted) "Grant" else null,
                onAction = onRequestNotificationPermission,
            )
        }
    }
}

@Composable
fun ChecklistItem(
    step: Int,
    title: String,
    description: String,
    isComplete: Boolean,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = if (isComplete)
                MaterialTheme.colorScheme.surfaceVariant
            else
                MaterialTheme.colorScheme.surface,
        ),
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                if (isComplete) "✓" else "$step",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = if (isComplete)
                    MaterialTheme.colorScheme.primary
                else
                    MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.width(28.dp),
            )
            Column(modifier = Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
                Text(
                    description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (actionLabel != null && onAction != null && !isComplete) {
                TextButton(onClick = onAction) {
                    Text(actionLabel)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Paired device card
// ---------------------------------------------------------------------------

@Composable
fun PairedDeviceCard(device: PairedDevice, onRemove: () -> Unit) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(device.name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
                Text(
                    "Paired ${formatTimestamp(device.pairedAt)}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            TextButton(onClick = onRemove) {
                Text("Remove", color = MaterialTheme.colorScheme.error)
            }
        }
    }
}

fun formatTimestamp(millis: Long): String {
    val diff = System.currentTimeMillis() - millis
    return when {
        diff < 60_000 -> "just now"
        diff < 3_600_000 -> "${diff / 60_000}m ago"
        diff < 86_400_000 -> "${diff / 3_600_000}h ago"
        else -> "${diff / 86_400_000}d ago"
    }
}
