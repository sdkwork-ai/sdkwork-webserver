package com.sdkwork.webserver.backend.sdk

data class ServerDirectoryListing(
    val nodeId: String? = null,
    val path: String? = null,
    val parentPath: String? = null,
    val entries: List<ServerEntry>? = null
)
