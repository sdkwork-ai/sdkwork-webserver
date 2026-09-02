package com.sdkwork.webserver.backend.sdk

data class CreateManagedDomainRequest(
    val hostname: String? = null,
    val applicationId: String? = null,
    val isPrimary: Boolean? = null,
    val sslEnabled: Boolean? = null,
    val sslProvider: String? = null
)
