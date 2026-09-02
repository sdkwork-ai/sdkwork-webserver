package com.sdkwork.webserver.backend.sdk

data class PageInfo(
    val mode: String? = null,
    val page: Int? = null,
    val pageSize: Int? = null,
    val totalItems: String? = null,
    val totalPages: Int? = null,
    val nextCursor: String? = null,
    val hasMore: Boolean? = null
)
