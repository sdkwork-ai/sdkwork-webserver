package com.sdkwork.webserver.backend.sdk

data class CertificateResponse(
    val id: String? = null,
    val certName: String? = null,
    val identifiers: List<CertificateIdentifierResponse>? = null,
    val certType: Int? = null,
    val issuer: String? = null,
    val fingerprint: String? = null,
    val keyAlgorithm: String? = null,
    val notBefore: String? = null,
    val notAfter: String? = null,
    val autoRenew: Boolean? = null,
    val renewalStatus: String? = null,
    val status: String? = null,
    val createdAt: String? = null
)
