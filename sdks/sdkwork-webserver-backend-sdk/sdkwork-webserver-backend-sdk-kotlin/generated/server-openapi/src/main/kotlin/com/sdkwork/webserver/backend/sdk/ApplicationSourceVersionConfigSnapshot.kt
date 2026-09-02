package com.sdkwork.webserver.backend.sdk

data class ApplicationSourceVersionConfigSnapshot(
    val appConfigPath: String? = null,
    val deploymentConfigPath: String? = null,
    val appConfigDetected: Boolean? = null,
    val deploymentConfigDetected: Boolean? = null
)
