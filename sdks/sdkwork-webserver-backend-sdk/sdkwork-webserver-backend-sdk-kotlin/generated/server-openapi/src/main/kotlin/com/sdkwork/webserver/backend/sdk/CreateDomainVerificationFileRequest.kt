package com.sdkwork.webserver.backend.sdk

data class CreateDomainVerificationFileRequest(
    val platform: String? = null,
    val fileName: String? = null,
    val content: String? = null
)
