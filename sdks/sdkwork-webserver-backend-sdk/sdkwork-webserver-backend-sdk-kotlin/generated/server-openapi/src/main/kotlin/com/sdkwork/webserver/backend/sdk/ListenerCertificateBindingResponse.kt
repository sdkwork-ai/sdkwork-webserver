package com.sdkwork.webserver.backend.sdk

data class ListenerCertificateBindingResponse(
    val id: String? = null,
    val siteId: String? = null,
    val domainId: String? = null,
    val certificateId: String? = null,
    val desiredCertificateVersionId: String? = null,
    val currentCertificateVersionId: String? = null,
    val desiredCertificate: ListenerCertificateSummaryResponse? = null,
    val currentCertificate: ListenerCertificateSummaryResponse? = null,
    val keyAlgorithm: String? = null,
    val priority: Int? = null,
    val isDefault: Boolean? = null,
    val status: String? = null,
    val activatedAt: String? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
