package com.sdkwork.webserver.app.sdk

data class CreateSourceVersionRequest(
    val versionTag: String? = null,
    val sourceType: String? = null,
    val sourceRef: String? = null,
    val commitHash: String? = null,
    val artifactDriveUri: String? = null,
    val artifactSize: String? = null,
    val artifactHash: String? = null,
    val configSnapshot: SourceVersionConfigSnapshot? = null
)
