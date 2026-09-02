package com.sdkwork.webserver.app.sdk

data class CreateEnvVariableRequest(
    val key: String? = null,
    val value_: String? = null,
    val environment: String? = null,
    val isSecret: Boolean? = null
)
