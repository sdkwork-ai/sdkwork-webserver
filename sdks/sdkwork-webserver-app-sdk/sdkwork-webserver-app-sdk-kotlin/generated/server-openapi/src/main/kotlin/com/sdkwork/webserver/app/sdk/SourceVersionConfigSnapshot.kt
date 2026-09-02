package com.sdkwork.webserver.app.sdk

data class SourceVersionConfigSnapshot(
    val appConfigPath: String? = null,
    val deploymentConfigPath: String? = null,
    val appConfigDetected: Boolean? = null,
    val deploymentConfigDetected: Boolean? = null
)
