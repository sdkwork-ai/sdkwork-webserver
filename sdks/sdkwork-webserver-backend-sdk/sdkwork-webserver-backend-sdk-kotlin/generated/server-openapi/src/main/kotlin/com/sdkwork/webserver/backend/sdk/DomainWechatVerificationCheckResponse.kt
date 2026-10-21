package com.sdkwork.webserver.backend.sdk

data class DomainWechatVerificationCheckResponse(
    val reachable: Boolean? = null,
    val matched: Boolean? = null,
    val statusCode: Int? = null,
    val scheme: String? = null,
    val detail: String? = null,
    val checkedAt: String? = null
)
