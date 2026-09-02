package com.sdkwork.webserver.app.sdk

data class CreateHealthCheckRequest(
    val checkType: Int? = null,
    val checkUrl: String? = null,
    val checkInterval: Int? = null,
    val timeoutMs: Int? = null,
    val retryCount: Int? = null
)
