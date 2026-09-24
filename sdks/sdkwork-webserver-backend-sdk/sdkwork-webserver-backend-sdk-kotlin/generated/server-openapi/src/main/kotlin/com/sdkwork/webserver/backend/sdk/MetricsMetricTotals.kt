package com.sdkwork.webserver.backend.sdk

data class MetricsMetricTotals(
    val metric: String? = null,
    val unit: String? = null,
    val values: List<MetricsWindowValue>? = null
)
