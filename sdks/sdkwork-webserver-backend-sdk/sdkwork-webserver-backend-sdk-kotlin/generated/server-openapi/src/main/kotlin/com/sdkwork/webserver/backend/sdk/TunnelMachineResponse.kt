package com.sdkwork.webserver.backend.sdk

data class TunnelMachineResponse(
    val deviceId: String? = null,
    val deviceName: String? = null,
    val platform: String? = null,
    val online: Boolean? = null,
    val enabled: Boolean? = null,
    val sessionId: String? = null,
    val firstSeenAt: String? = null,
    val lastSeenAt: String? = null,
    val connectedAt: String? = null,
    val disconnectedAt: String? = null,
    val remoteAddr: String? = null,
    val bytesIn: String? = null,
    val bytesOut: String? = null,
    val sessionsTotal: String? = null,
    val routeCount: String? = null,
    val healthyRouteCount: String? = null,
    val unhealthyRouteIds: List<String>? = null,
    val routes: List<TunnelMachineRouteResponse>? = null
)
