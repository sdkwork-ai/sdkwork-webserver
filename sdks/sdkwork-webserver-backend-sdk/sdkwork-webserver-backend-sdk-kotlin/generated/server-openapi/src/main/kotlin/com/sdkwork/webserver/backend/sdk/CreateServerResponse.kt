package com.sdkwork.webserver.backend.sdk

data class CreateServerResponse(
    val id: String? = null,
    val name: String? = null,
    val host: String? = null,
    val tenantScopeHash: String? = null,
    val sshPort: Int? = null,
    val status: Int? = null,
    val lastHeartbeatAt: String? = null,
    val createdAt: String? = null,
    val agentToken: String? = null
)
