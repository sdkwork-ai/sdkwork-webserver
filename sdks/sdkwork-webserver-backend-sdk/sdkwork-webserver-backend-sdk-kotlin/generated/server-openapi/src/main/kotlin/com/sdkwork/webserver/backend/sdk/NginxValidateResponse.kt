package com.sdkwork.webserver.backend.sdk

data class NginxValidateResponse(
    val valid: Boolean? = null,
    val errors: List<Map<String, Any>>? = null
)
