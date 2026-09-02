package com.sdkwork.webserver.backend.sdk

data class RootDomainResponse(
    val id: String? = null,
    val hostname: String? = null,
    val status: Int? = null,
    val subdomainCount: String? = null,
    val boundSubdomainCount: String? = null,
    val verifiedSubdomainCount: String? = null,
    val httpsSubdomainCount: String? = null,
    val activeDeploymentCount: String? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
