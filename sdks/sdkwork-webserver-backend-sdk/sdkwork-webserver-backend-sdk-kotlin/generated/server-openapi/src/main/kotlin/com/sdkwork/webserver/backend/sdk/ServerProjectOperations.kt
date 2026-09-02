package com.sdkwork.webserver.backend.sdk

data class ServerProjectOperations(
    val nodeId: String? = null,
    val path: String? = null,
    val projectType: String? = null,
    val operations: List<ServerProjectOperation>? = null
)
