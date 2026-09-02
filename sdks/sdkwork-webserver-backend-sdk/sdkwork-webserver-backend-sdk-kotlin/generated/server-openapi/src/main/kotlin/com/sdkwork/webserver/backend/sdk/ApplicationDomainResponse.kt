package com.sdkwork.webserver.backend.sdk

data class ApplicationDomainResponse(
    val id: String? = null,
    val hostname: String? = null,
    val rootDomainId: String? = null,
    val recordName: String? = null,
    val applicationId: String? = null,
    val applicationName: String? = null,
    val certificateCount: String? = null,
    val isPrimary: Boolean? = null,
    val isVerified: Boolean? = null,
    val sslEnabled: Boolean? = null,
    val sslProvider: String? = null,
    val status: Int? = null,
    val latestDeployment: DomainDeploymentResponse? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
