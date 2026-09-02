package com.sdkwork.webserver.backend.sdk

data class CertificateIdentifierResponse(
    val domainId: String? = null,
    val hostname: String? = null,
    val identifierType: String? = null,
    val position: Int? = null
)
