package com.sdkwork.webserver.app.sdk

data class ApplicationStoreListing(
    val icon: MediaResource? = null,
    val cover: MediaResource? = null,
    val previews: List<MediaResource>? = null,
    val shortDescription: String? = null,
    val fullDescription: String? = null,
    val releaseNotes: String? = null,
    val category: String? = null,
    val keywords: List<String>? = null,
    val supportUrl: String? = null,
    val privacyPolicyUrl: String? = null,
    val officialWebsiteUrl: String? = null
)
