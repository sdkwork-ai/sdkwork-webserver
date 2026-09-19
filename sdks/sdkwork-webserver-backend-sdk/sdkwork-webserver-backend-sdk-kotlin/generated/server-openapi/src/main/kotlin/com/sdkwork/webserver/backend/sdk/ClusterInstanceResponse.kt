package com.sdkwork.webserver.backend.sdk

data class ClusterInstanceResponse(
    val id: String? = null,
    val clusterId: String? = null,
    val hostId: String? = null,
    val hostName: String? = null,
    val name: String? = null,
    val role: String? = null,
    val environment: String? = null,
    val processPid: Int? = null,
    val processStartedAt: String? = null,
    val bindHost: String? = null,
    val bindPort: Int? = null,
    val publicEndpoint: String? = null,
    val buildVersion: String? = null,
    val status: Int? = null,
    val healthState: String? = null,
    val lastHeartbeatAt: String? = null,
    val lastOnlineAt: String? = null,
    val uptimeSeconds: String? = null,
    val metrics: Map<String, Any>? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
