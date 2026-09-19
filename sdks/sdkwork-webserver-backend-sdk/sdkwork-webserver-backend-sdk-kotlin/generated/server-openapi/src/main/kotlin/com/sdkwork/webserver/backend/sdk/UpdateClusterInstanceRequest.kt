package com.sdkwork.webserver.backend.sdk

data class UpdateClusterInstanceRequest(
    val name: String? = null,
    val status: Int? = null,
    val publicEndpoint: String? = null
)
