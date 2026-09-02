package com.sdkwork.webserver.backend.sdk

data class ServerFilesNode(
    val id: String? = null,
    val name: String? = null,
    val host: String? = null,
    val sshPort: Int? = null,
    val status: String? = null,
    val filesystemRoot: String? = null,
    val region: String? = null
)
