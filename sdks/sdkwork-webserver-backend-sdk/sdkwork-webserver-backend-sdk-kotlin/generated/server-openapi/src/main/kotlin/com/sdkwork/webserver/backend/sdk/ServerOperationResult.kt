package com.sdkwork.webserver.backend.sdk

data class ServerOperationResult(
    val operationId: String? = null,
    val exitCode: Int? = null,
    val stdout: String? = null,
    val stderr: String? = null
)
