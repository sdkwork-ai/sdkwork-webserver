package com.sdkwork.webserver.backend.sdk

data class CertificateOperationResponse(
    val id: String? = null,
    val certificateId: String? = null,
    val operationType: String? = null,
    val status: String? = null,
    val attemptCount: Int? = null,
    val maxAttempts: Int? = null,
    val nextAttemptAt: String? = null,
    val failureCode: String? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null,
    val completedAt: String? = null
)
