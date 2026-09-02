package com.sdkwork.webserver.app.sdk

data class HealthCheckResponse(
    val id: String? = null,
    val checkType: Int? = null,
    val checkUrl: String? = null,
    val checkInterval: Int? = null,
    val timeoutMs: Int? = null,
    val retryCount: Int? = null,
    val status: Int? = null,
    val createdAt: String? = null
)
