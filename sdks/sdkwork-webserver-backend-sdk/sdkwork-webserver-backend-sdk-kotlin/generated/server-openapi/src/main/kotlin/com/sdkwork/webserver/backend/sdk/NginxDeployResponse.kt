package com.sdkwork.webserver.backend.sdk

data class NginxDeployResponse(
    val success: Boolean? = null,
    val configId: String? = null,
    val deployedAt: String? = null,
    val reloadResult: Map<String, Any>? = null
)
