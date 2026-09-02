package com.sdkwork.webserver.backend.sdk

data class UpdateNginxConfigRequest(
    val configContent: String? = null,
    val configName: String? = null
)
