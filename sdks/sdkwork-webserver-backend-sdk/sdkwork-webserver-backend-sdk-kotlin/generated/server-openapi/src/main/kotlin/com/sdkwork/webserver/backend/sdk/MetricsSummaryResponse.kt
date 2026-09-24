package com.sdkwork.webserver.backend.sdk

data class MetricsSummaryResponse(
    val asOf: String? = null,
    val platformScope: Boolean? = null,
    val trafficSince: String? = null,
    val windows: List<MetricsWindowBounds>? = null,
    val entities: List<MetricsMetricTotals>? = null,
    val traffic: List<MetricsMetricTotals>? = null,
    val storage: List<MetricsMetricTotals>? = null,
    val series: List<MetricsSeries>? = null,
    val seriesWindow: MetricsSeriesWindow? = null,
    val unassembledMetrics: List<String>? = null
)
