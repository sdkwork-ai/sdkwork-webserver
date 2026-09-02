package com.sdkwork.webserver.backend.sdk

data class IssueCertificateRequest(
    val domainIds: List<String>? = null,
    val certType: Int? = null,
    val keyAlgorithm: String? = null,
    val autoRenew: Boolean? = null
)
