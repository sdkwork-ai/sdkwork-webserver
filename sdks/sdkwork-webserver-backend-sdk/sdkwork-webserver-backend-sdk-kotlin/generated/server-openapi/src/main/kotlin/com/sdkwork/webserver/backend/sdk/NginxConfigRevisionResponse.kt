package com.sdkwork.webserver.backend.sdk

data class NginxConfigRevisionResponse(
    val versionNo: Int? = null,
    val configName: String? = null,
    val configContent: String? = null,
    val configHash: String? = null,
    val createdBy: String? = null,
    val createdAt: String? = null
)
