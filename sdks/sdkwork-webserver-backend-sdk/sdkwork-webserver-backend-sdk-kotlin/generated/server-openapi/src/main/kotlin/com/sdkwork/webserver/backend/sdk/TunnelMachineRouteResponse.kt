package com.sdkwork.webserver.backend.sdk

data class TunnelMachineRouteResponse(
    val routeId: String? = null,
    val name: String? = null,
    val protocol: String? = null,
    val domain: String? = null,
    val port: Int? = null,
    val secretName: String? = null,
    val locations: List<String>? = null,
    val loadBalancerGroup: String? = null,
    val bandwidthLimitBytesPerSecond: String? = null,
    val proxyProtocol: String? = null,
    val allowPublic: Boolean? = null,
    val auth: String? = null,
    val allowedIps: List<String>? = null,
    val healthy: Boolean? = null
)
