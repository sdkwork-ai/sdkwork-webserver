package com.sdkwork.webserver.app.sdk

data class PageInfo(
    val mode: String? = null,
    val page: Int? = null,
    val pageSize: Int? = null,
    val totalItems: String? = null,
    val totalPages: Int? = null,
    val nextCursor: String? = null,
    val hasMore: Boolean? = null
)
