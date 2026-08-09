package com.sdkwork.web.app.sdk

data class DeploymentResponse(
    val id: String? = null,
    val applicationId: String? = null,
    val deployType: Int? = null,
    val sourceVersionId: String? = null,
    val versionTag: String? = null,
    val commitHash: String? = null,
    val sourceRef: String? = null,
    val rollbackFromDeploymentId: String? = null,
    val environment: String? = null,
    val artifactDriveUri: String? = null,
    val artifactSize: String? = null,
    val artifactHash: String? = null,
    val status: Int? = null,
    val startedAt: String? = null,
    val completedAt: String? = null,
    val durationMs: String? = null,
    val createdAt: String? = null
)
