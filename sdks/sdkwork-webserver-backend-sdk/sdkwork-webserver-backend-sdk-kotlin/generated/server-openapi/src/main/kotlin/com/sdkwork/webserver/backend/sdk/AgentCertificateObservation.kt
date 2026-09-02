package com.sdkwork.webserver.backend.sdk

data class AgentCertificateObservation(
    val certificateId: String? = null,
    val fingerprint: String? = null,
    val syncVersion: String? = null,
    val state: String? = null,
    val observedAt: String? = null,
    val failureCode: String? = null
)
