package com.sdkwork.webserver.backend.sdk

data class DomainDnsSyncResponse(
    val recordCount: String? = null,
    val syncedAt: String? = null,
    val zoneApex: String? = null,
    val dnsProvider: String? = null,
    val cloudAccountId: String? = null
)
