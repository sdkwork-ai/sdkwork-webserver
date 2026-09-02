package com.sdkwork.webserver.backend.sdk

data class CreateNginxConfigRequest(
    val configType: Int? = null,
    val configName: String? = null,
    val configContent: String? = null,
    val siteId: String? = null
)
