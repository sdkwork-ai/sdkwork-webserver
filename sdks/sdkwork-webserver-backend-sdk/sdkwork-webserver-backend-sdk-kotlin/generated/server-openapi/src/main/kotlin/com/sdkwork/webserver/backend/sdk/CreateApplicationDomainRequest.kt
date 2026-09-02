package com.sdkwork.webserver.backend.sdk

data class CreateApplicationDomainRequest(
    val hostname: String? = null,
    val isPrimary: Boolean? = null,
    val sslEnabled: Boolean? = null,
    val sslProvider: String? = null
)
