package com.sdkwork.webserver.backend.sdk

data class WebserverConfigWriteRequest(
    val content: String? = null,
    val expectedSha256: String? = null
)
