package com.sdkwork.webserver.backend.sdk

data class DomainDeploymentResponse(
    val id: String? = null,
    val status: Int? = null,
    val environment: String? = null,
    val versionTag: String? = null,
    val completedAt: String? = null,
    val createdAt: String? = null
)
