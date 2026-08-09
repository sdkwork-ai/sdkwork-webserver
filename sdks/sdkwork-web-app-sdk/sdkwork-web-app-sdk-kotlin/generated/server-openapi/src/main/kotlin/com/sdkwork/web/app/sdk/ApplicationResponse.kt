package com.sdkwork.web.app.sdk

data class ApplicationResponse(
    val id: String? = null,
    val name: String? = null,
    val slug: String? = null,
    val description: String? = null,
    val siteId: String? = null,
    val applicationType: String? = null,
    val siteType: Int? = null,
    val status: Int? = null,
    val runtimeConfig: Map<String, Any>? = null,
    val storeListing: ApplicationStoreListing? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
