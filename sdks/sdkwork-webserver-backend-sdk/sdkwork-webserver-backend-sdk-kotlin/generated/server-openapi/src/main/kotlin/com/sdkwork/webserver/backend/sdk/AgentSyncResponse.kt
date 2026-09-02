package com.sdkwork.webserver.backend.sdk

data class AgentSyncResponse(
    val serverId: String? = null,
    val syncVersion: String? = null,
    val unchanged: Boolean? = null,
    val nginxConfigs: List<AgentNginxConfigBundle>? = null,
    val certificates: List<AgentCertificateBundle>? = null
)
