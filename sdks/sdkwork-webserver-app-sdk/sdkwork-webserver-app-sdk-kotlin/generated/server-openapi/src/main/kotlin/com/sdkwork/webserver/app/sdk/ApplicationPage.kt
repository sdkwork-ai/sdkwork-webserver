package com.sdkwork.webserver.app.sdk

data class ApplicationPage(
    val items: List<ApplicationResponse>? = null,
    val total: String? = null,
    val page: Int? = null,
    val pageSize: Int? = null
)
