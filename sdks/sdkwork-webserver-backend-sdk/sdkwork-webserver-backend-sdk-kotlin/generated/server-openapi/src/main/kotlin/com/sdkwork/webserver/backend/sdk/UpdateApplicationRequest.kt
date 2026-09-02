package com.sdkwork.webserver.backend.sdk

data class UpdateApplicationRequest(
    val name: String? = null,
    val description: String? = null,
    val runtimeConfig: Map<String, Any>? = null,
    val storeListing: ApplicationStoreListing? = null
)
