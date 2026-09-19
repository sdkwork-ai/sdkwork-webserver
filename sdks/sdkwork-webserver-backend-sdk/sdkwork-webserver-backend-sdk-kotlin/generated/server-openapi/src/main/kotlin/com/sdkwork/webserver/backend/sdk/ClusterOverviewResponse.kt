package com.sdkwork.webserver.backend.sdk

data class ClusterOverviewResponse(
    val totalHosts: String? = null,
    val onlineHosts: String? = null,
    val totalInstances: String? = null,
    val onlineInstances: String? = null,
    val unhealthyInstances: String? = null,
    val pendingPeerMessages: String? = null,
    val generatedAt: String? = null
)
