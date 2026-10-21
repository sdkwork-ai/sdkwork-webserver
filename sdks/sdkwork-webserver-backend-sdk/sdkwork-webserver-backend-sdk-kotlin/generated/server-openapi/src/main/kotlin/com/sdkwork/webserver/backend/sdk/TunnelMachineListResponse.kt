package com.sdkwork.webserver.backend.sdk

data class TunnelMachineListResponse(
    val machines: List<TunnelMachineResponse>? = null,
    val pageInfo: PageInfo? = null
)
