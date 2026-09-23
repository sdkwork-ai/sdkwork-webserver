package com.sdkwork.webserver.backend.sdk

data class UpdateClusterInstanceRequest(
    val name: String? = null,
    val status: Int? = null,
    val publicEndpoint: String? = null,
    val routingEnabled: Boolean? = null,
    val draining: Boolean? = null,
    val probeUrl: String? = null,
    val labels: Map<String, String>? = null,
    val routingWeight: Int? = null,
    val maintenanceNote: String? = null
)
