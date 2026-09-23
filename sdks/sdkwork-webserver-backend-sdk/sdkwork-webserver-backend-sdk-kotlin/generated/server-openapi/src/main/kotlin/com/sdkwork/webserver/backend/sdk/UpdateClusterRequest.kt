package com.sdkwork.webserver.backend.sdk

data class UpdateClusterRequest(
    val name: String? = null,
    val description: String? = null,
    val status: Int? = null,
    val heartbeatIntervalSeconds: Int? = null,
    val offlineThresholdSeconds: Int? = null,
    val lbStrategy: String? = null,
    val servedDomains: List<String>? = null
)
