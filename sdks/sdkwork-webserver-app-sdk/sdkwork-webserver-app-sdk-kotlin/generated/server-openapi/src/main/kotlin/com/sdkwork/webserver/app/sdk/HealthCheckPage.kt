package com.sdkwork.webserver.app.sdk

data class HealthCheckPage(
    val items: List<HealthCheckResponse>? = null,
    val total: String? = null
)
