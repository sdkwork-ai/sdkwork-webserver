package com.sdkwork.webserver.backend.sdk

data class WebserverConfigCatalog(
    val configRoot: String? = null,
    val items: List<WebserverConfigEntry>? = null
)
