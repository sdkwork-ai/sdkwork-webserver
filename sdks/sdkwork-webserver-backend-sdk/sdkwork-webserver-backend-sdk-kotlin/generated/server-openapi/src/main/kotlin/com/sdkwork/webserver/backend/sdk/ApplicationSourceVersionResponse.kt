package com.sdkwork.webserver.backend.sdk

data class ApplicationSourceVersionResponse(
    val id: String? = null,
    val siteId: String? = null,
    val versionTag: String? = null,
    val sourceType: String? = null,
    val sourceRef: String? = null,
    val commitHash: String? = null,
    val artifactDriveUri: String? = null,
    val artifactSize: String? = null,
    val artifactHash: String? = null,
    val configSnapshot: ApplicationSourceVersionConfigSnapshot? = null,
    val status: Int? = null,
    val retained: Boolean? = null,
    val createdAt: String? = null
)
