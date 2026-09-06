package com.sdkwork.webserver.backend.sdk

data class WebserverConfigFile(
    val id: String? = null,
    val kind: String? = null,
    val name: String? = null,
    val path: String? = null,
    val language: String? = null,
    val writable: Boolean? = null,
    val content: String? = null,
    val size: String? = null,
    val sha256: String? = null,
    val updatedAt: String? = null
)
