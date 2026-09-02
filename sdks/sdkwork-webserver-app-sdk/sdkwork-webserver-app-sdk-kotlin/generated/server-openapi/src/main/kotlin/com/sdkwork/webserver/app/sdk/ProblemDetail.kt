package com.sdkwork.webserver.app.sdk

data class ProblemDetail(
    val type: String? = null,
    val title: String? = null,
    val status: Int? = null,
    val detail: String? = null,
    val instance: String? = null,
    val code: Int? = null,
    val traceId: String? = null,
    val errors: List<FieldError>? = null
)
