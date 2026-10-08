package com.sdkwork.webserver.backend.sdk

data class UpdateDomainDnsRecordRequest(
    val recordType: String? = null,
    val host: String? = null,
    val recordValue: String? = null,
    val ttlSeconds: Int? = null,
    val priority: Int? = null,
    val recordLine: String? = null
)
