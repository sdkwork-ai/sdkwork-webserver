package com.sdkwork.webserver.backend.sdk

data class ServerEntry(
    val name: String? = null,
    val kind: String? = null,
    val path: String? = null,
    val size: String? = null,
    val projectType: String? = null,
    val isProjectRoot: Boolean? = null
)
