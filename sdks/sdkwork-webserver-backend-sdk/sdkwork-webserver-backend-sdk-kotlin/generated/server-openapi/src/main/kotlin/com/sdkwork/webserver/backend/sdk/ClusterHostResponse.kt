package com.sdkwork.webserver.backend.sdk

data class ClusterHostResponse(
    val id: String? = null,
    val clusterId: String? = null,
    val name: String? = null,
    val hostname: String? = null,
    val machineCode: String? = null,
    val osName: String? = null,
    val osVersion: String? = null,
    val kernelVersion: String? = null,
    val arch: String? = null,
    val cpuModel: String? = null,
    val cpuCores: Int? = null,
    val memoryTotalMb: String? = null,
    val remoteIp: String? = null,
    val localIps: List<String>? = null,
    val macAddresses: List<String>? = null,
    val daemonVersion: String? = null,
    val status: Int? = null,
    val lastHeartbeatAt: String? = null,
    val instanceCount: String? = null,
    val joinMode: String? = null,
    val tunnelRouteDomain: String? = null,
    val createdAt: String? = null,
    val updatedAt: String? = null
)
