package com.sdkwork.webserver.app.sdk

data class EnvVariableResponse(
    val id: String? = null,
    val key: String? = null,
    val environment: String? = null,
    val isSecret: Boolean? = null,
    val createdAt: String? = null
)
