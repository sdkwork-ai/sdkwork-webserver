package com.sdkwork.webserver.backend.sdk

data class AgentHeartbeatRequest(
    val agentVersion: String? = null,
    val nginxEnabled: Boolean? = null,
    val activeConfigs: String? = null,
    val lastSyncVersion: String? = null,
    val certificateObservations: List<AgentCertificateObservation>? = null
)
