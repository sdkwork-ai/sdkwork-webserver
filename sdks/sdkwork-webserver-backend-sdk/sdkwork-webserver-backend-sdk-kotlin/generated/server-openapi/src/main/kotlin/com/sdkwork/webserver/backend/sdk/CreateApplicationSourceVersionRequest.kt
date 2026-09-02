package com.sdkwork.webserver.backend.sdk

data class CreateApplicationSourceVersionRequest(
    val versionTag: String? = null,
    val sourceType: String? = null,
    val sourceRef: String? = null,
    val commitHash: String? = null,
    val artifactDriveUri: String? = null,
    val artifactSize: String? = null,
    val artifactHash: String? = null,
    val configSnapshot: ApplicationSourceVersionConfigSnapshot? = null
)
