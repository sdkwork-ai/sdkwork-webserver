package com.sdkwork.webserver.backend.sdk

data class DomainDnsRecordResponse(
    val id: String? = null,
    val recordName: String? = null,
    val recordType: String? = null,
    val recordValue: String? = null,
    val ttlSeconds: Int? = null,
    val priority: Int? = null,
    val recordLine: String? = null,
    val domainId: String? = null,
    val dnsProvider: String? = null,
    val cloudAccountId: String? = null,
    val providerRecordRef: String? = null,
    val syncedAt: String? = null
)
