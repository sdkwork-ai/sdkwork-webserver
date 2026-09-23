package com.sdkwork.webserver.backend.sdk

data class ClusterResponse(
    val id: String? = null,
    val name: String? = null,
    val code: String? = null,
    val description: String? = null,
    val status: Int? = null,
    val heartbeatIntervalSeconds: Int? = null,
    val offlineThresholdSeconds: Int? = null,
    val hostCount: String? = null,
    val instanceCount: String? = null,
    val onlineInstanceCount: String? = null,
    val lbStrategy: String? = null,
    val servedDomains: List<String>? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
