package com.sdkwork.webserver.backend.sdk

data class NginxStatusResponse(
    val running: Boolean? = null,
    val version: String? = null,
    val pid: Int? = null,
    val activeConnections: Int? = null,
    val configPath: String? = null,
    val uptime: String? = null
)
