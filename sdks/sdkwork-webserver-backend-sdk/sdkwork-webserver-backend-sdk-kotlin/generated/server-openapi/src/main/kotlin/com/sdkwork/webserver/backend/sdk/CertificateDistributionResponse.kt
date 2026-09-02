package com.sdkwork.webserver.backend.sdk

data class CertificateDistributionResponse(
    val serverId: String? = null,
    val serverName: String? = null,
    val host: String? = null,
    val desiredSyncVersion: String? = null,
    val appliedSyncVersion: String? = null,
    val status: String? = null,
    val lastHeartbeatAt: String? = null
)
