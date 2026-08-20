package com.sdkwork.web.app.sdk

data class PlatformTargetResponse(
    val id: String? = null,
    val appId: String? = null,
    val targetKey: String? = null,
    val platform: String? = null,
    val techStack: String? = null,
    val architectures: List<String>? = null,
    val bundleId: String? = null,
    val packageName: String? = null,
    val appIdValue: String? = null,
    val bundleName: String? = null,
    val targetStatus: String? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
