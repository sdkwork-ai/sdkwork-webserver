package com.sdkwork.webserver.backend.sdk

data class EnqueueClusterPeerMessagesRequest(
    val clusterId: String? = null,
    val toInstanceId: String? = null,
    val fromInstanceId: String? = null,
    val messageType: String? = null,
    val payload: Map<String, Any>? = null,
    val expiresInSeconds: Int? = null
)
