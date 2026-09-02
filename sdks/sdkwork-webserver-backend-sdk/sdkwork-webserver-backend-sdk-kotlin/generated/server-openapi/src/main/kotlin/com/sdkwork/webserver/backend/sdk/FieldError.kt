package com.sdkwork.webserver.backend.sdk

data class FieldError(
    val field_: String? = null,
    val message: String? = null,
    val code: Int? = null
)
