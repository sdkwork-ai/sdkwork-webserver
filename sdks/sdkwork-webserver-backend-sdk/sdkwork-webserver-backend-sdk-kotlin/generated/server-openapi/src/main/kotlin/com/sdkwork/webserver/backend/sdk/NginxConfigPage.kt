package com.sdkwork.webserver.backend.sdk

data class NginxConfigPage(
    val items: List<NginxConfigResponse>? = null,
    val total: String? = null
)
