package com.sdkwork.webserver.backend.sdk

data class ClusterHeartbeatSampleResponse(
    val id: String? = null,
    val status: Int? = null,
    val latencyMs: Int? = null,
    val metrics: Map<String, Any>? = null,
    val reportedAt: String? = null
)
