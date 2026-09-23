package com.sdkwork.webserver.backend.sdk

data class CreateClusterRequest(
    val name: String? = null,
    val code: String? = null,
    val description: String? = null,
    val heartbeatIntervalSeconds: Int? = null,
    val offlineThresholdSeconds: Int? = null,
    val lbStrategy: String? = null,
    val servedDomains: List<String>? = null
)
