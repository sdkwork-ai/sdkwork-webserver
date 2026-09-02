package com.sdkwork.webserver.app.sdk

data class CreateDomainRequest(
    val hostname: String? = null,
    val isPrimary: Boolean? = null,
    val sslEnabled: Boolean? = null,
    val sslProvider: String? = null
)
