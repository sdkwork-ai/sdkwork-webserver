package com.sdkwork.webserver.backend.sdk

data class ServerPage(
    val items: List<ServerResponse>? = null,
    val total: String? = null
)
