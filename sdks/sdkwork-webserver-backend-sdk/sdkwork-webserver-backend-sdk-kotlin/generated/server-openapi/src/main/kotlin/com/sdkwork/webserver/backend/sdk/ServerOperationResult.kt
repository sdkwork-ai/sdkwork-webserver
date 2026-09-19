package com.sdkwork.webserver.backend.sdk

data class ServerOperationResult(
    val operationId: String? = null,
    val exitCode: Int? = null,
    val timedOut: Boolean? = null,
    val stdout: String? = null,
    val stderr: String? = null,
    val stdoutTruncated: Boolean? = null,
    val stderrTruncated: Boolean? = null,
    val pid: Int? = null,
    val pidFile: String? = null,
    val logFile: String? = null,
    val stopped: Boolean? = null,
    val message: String? = null
)
