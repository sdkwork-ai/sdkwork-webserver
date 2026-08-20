package com.sdkwork.web.app.sdk

data class CreateApplicationRequest(
    val name: String? = null,
    val slug: String? = null,
    val description: String? = null,
    val appKind: String? = null,
    val runtimeConfig: Map<String, Any>? = null,
    val storeListing: ApplicationStoreListing? = null
)
