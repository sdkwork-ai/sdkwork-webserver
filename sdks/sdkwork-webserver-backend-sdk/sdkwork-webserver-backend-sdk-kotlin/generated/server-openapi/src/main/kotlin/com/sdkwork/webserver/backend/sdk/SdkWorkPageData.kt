package com.sdkwork.webserver.backend.sdk

data class SdkWorkPageData(
    val items: List<Map<String, Any>>? = null,
    val pageInfo: PageInfo? = null
)
