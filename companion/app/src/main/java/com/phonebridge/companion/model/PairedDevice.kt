package com.phonebridge.companion.model

import kotlinx.serialization.Serializable

@Serializable
data class PairedDevice(
    val id: String,
    val name: String,
    val publicKey: String,
    val pairedAt: Long,
    val lastSeen: Long,
)
