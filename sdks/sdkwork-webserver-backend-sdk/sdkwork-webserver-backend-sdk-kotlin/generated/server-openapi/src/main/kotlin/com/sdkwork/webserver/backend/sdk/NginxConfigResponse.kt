package com.sdkwork.webserver.backend.sdk

data class NginxConfigResponse(
    val id: String? = null,
    val configType: Int? = null,
    val configName: String? = null,
    val configContent: String? = null,
    val configHash: String? = null,
    val isActive: Boolean? = null,
    val versionNo: Int? = null,
    val deployedAt: String? = null,
    val status: Int? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
