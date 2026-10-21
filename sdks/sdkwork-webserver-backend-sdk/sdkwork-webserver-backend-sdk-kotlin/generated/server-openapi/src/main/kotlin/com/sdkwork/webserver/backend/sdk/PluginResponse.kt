package com.sdkwork.webserver.backend.sdk

data class PluginResponse(
    val id: String? = null,
    val pluginKey: String? = null,
    val displayName: String? = null,
    val summary: String? = null,
    val version: String? = null,
    val categoryId: String? = null,
    val supportedHostTools: List<String>? = null,
    val contributedCapabilities: List<String>? = null,
    val sourceKind: String? = null,
    val gitRepository: String? = null,
    val gitRef: String? = null,
    val artifactRef: String? = null,
    val checksumSha256: String? = null,
    val archiveFileName: String? = null,
    val status: String? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
