package com.sdkwork.webserver.app.sdk

data class ListenerCertificateSummaryResponse(
    val certName: String? = null,
    val identifiers: List<CertificateIdentifierResponse>? = null,
    val issuer: String? = null,
    val fingerprint: String? = null,
    val notAfter: String? = null,
    val status: String? = null
)
