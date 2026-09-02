package com.sdkwork.webserver.app.sdk

data class CreatePlatformTargetRequest(
    val targetKey: String? = null,
    val platform: String? = null,
    val techStack: String? = null,
    val architectures: List<String>? = null,
    val bundleId: String? = null,
    val packageName: String? = null,
    val appId: String? = null,
    val bundleName: String? = null,
    val allowedChannels: List<String>? = null
)
