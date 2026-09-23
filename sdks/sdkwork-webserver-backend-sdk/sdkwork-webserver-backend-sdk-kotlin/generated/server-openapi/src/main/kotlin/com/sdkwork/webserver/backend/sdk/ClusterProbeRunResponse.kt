package com.sdkwork.webserver.backend.sdk

data class ClusterProbeRunResponse(
    val healthy: Boolean? = null,
    val latencyMs: Int? = null,
    val failures: Int? = null,
    val ejected: Boolean? = null,
    val recovered: Boolean? = null
)
