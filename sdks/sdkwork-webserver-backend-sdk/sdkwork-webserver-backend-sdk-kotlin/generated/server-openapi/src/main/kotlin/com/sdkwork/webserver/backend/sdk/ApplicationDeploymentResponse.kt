package com.sdkwork.webserver.backend.sdk

data class ApplicationDeploymentResponse(
    val id: String? = null,
    val siteId: String? = null,
    val sourceVersionId: String? = null,
    val status: Int? = null,
    val deployType: Int? = null,
    val environment: String? = null,
    val versionTag: String? = null,
    val commitHash: String? = null,
    val sourceRef: String? = null,
    val rollbackFromDeploymentId: String? = null,
    val artifactDriveUri: String? = null,
    val artifactSize: String? = null,
    val artifactHash: String? = null,
    val startedAt: String? = null,
    val completedAt: String? = null,
    val durationMs: String? = null,
    val createdAt: String? = null
)
