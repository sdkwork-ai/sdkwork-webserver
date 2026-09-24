package com.sdkwork.webserver.backend.sdk

data class MetricsSeries(
    val metric: String? = null,
    val unit: String? = null,
    val points: List<MetricsSeriesPoint>? = null
)
