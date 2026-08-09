package com.sdkwork.web.app.sdk

data class CreateApplicationRequest(
    val name: String? = null,
    val slug: String? = null,
    val description: String? = null,
    val applicationType: String? = null,
    val siteType: Int? = null,
    val runtimeConfig: Map<String, Any>? = null,
    val storeListing: ApplicationStoreListing? = null
)
