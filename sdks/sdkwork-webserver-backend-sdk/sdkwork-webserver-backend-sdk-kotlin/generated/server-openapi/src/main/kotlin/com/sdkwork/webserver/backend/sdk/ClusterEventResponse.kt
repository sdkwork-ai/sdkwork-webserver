package com.sdkwork.webserver.backend.sdk

data class ClusterEventResponse(
    val id: String? = null,
    val clusterId: String? = null,
    val hostId: String? = null,
    val instanceId: String? = null,
    val eventType: String? = null,
    val severity: String? = null,
    val message: String? = null,
    val detail: Map<String, Any>? = null,
    val occurredAt: String? = null,
    val createdAt: String? = null
)
