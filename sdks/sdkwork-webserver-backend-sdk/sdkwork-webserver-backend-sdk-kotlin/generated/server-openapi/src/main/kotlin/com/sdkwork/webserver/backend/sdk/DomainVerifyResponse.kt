package com.sdkwork.webserver.backend.sdk

data class DomainVerifyResponse(
    val verified: Boolean? = null,
    val status: String? = null,
    val method: String? = null,
    val recordName: String? = null,
    val recordValue: String? = null,
    val attemptCount: Int? = null,
    val expiresAt: String? = null,
    val nextAttemptAt: String? = null,
    val checkedAt: String? = null,
    val failureCode: String? = null
)
