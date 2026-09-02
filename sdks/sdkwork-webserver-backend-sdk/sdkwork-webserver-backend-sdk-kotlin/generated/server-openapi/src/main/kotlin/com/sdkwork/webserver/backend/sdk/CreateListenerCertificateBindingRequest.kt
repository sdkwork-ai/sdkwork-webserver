package com.sdkwork.webserver.backend.sdk

data class CreateListenerCertificateBindingRequest(
    val certificateId: String? = null,
    val certificateVersionId: String? = null,
    val priority: Int? = null,
    val isDefault: Boolean? = null
)
