package com.sdkwork.webserver.app.sdk

data class MediaResource(
    val id: String? = null,
    val kind: String? = null,
    val source: String? = null,
    val url: String? = null,
    val publicUrl: String? = null,
    val uri: String? = null,
    val objectBlobId: String? = null,
    val fileName: String? = null,
    val mimeType: String? = null,
    val sizeBytes: String? = null,
    val checksum: MediaChecksum? = null,
    val width: Int? = null,
    val height: Int? = null,
    val durationSeconds: Double? = null,
    val altText: String? = null,
    val title: String? = null,
    val metadata: Map<String, Any>? = null
)
