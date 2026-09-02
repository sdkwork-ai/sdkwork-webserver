package com.sdkwork.webserver.app.sdk

data class SourceVersionResponse(
    val id: String? = null,
    val applicationId: String? = null,
    val versionTag: String? = null,
    val sourceType: String? = null,
    val sourceRef: String? = null,
    val commitHash: String? = null,
    val artifactDriveUri: String? = null,
    val artifactSize: String? = null,
    val artifactHash: String? = null,
    val configSnapshot: SourceVersionConfigSnapshot? = null,
    val status: Int? = null,
    val retained: Boolean? = null,
    val createdAt: String? = null
)
