package com.sdkwork.webserver.app.sdk

data class ImportGitSourceVersionRequest(
    val versionTag: String? = null,
    val repositoryUrl: String? = null,
    val gitRef: String? = null
)
