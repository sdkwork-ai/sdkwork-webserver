package com.sdkwork.webserver.backend.sdk

data class CreateTunnelMachineRouteRequest(
    val name: String? = null,
    val protocol: String? = null,
    val domain: String? = null,
    val port: Int? = null,
    val allowPublic: Boolean? = null,
    val allowedIps: List<String>? = null,
    val auth: String? = null,
    val visitorTokens: List<String>? = null,
    val locations: List<String>? = null,
    val bandwidthLimitBytesPerSecond: String? = null,
    val proxyProtocol: String? = null,
    val loadBalancerGroup: String? = null
)
