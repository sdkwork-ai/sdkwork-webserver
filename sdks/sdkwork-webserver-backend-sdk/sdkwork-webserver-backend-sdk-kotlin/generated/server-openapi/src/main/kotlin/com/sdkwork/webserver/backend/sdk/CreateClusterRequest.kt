package com.sdkwork.webserver.backend.sdk

data class CreateClusterRequest(
    val name: String? = null,
    val code: String? = null,
    val description: String? = null,
    val heartbeatIntervalSeconds: Int? = null,
    val offlineThresholdSeconds: Int? = null
)
