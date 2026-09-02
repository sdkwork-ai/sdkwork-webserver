package com.sdkwork.webserver.app.sdk

data class FieldError(
    val field_: String? = null,
    val message: String? = null,
    val code: Int? = null
)
