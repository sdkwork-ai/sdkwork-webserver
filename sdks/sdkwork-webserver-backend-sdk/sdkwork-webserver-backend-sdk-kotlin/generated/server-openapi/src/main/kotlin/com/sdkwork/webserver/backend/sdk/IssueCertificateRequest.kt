package com.sdkwork.webserver.backend.sdk

data class IssueCertificateRequest(
    val domainIds: List<String>? = null,
    val certType: Int? = null,
    val keyAlgorithm: String? = null,
    val autoRenew: Boolean? = null,
    val certName: String? = null,
    val certificateScope: String? = null,
    val validationMethod: String? = null,
    val renewBeforeDays: Int? = null,
    val caProfile: String? = null,
    val providerAccountId: String? = null
)
