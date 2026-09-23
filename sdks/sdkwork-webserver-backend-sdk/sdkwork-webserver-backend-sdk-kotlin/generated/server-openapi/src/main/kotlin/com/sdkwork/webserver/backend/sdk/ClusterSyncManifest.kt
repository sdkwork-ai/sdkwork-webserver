package com.sdkwork.webserver.backend.sdk

data class ClusterSyncManifest(
    val clusterId: String? = null,
    val kind: String? = null,
    val revision: String? = null,
    val sha256: String? = null,
    val payload: Map<String, Any>? = null,
    val createdAt: String? = null
)
