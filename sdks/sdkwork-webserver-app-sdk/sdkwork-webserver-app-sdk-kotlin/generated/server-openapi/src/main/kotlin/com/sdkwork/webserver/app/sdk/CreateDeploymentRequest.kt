package com.sdkwork.webserver.app.sdk

data class CreateDeploymentRequest(
    val sourceVersionId: String? = null,
    val deployType: Int? = null,
    val versionTag: String? = null,
    val commitHash: String? = null,
    val sourceRef: String? = null,
    val artifactDriveUri: String? = null,
    val artifactSize: String? = null,
    val artifactHash: String? = null,
    val environment: String? = null
)
