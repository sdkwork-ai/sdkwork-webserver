package com.sdkwork.webserver.backend.sdk

data class UpdateRootDomainRequest(
    val displayName: String? = null,
    val dnsProvider: String? = null,
    val providerZoneRef: String? = null,
    val status: Int? = null
)
