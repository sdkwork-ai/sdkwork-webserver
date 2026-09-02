package com.sdkwork.webserver.backend.sdk

data class CreateServerRequest(
    val name: String? = null,
    val host: String? = null,
    val tenantScopeHash: String? = null,
    val sshPort: Int? = null
)
