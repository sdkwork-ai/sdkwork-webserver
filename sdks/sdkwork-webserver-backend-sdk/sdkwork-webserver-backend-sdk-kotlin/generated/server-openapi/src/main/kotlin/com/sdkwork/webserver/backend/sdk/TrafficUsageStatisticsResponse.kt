package com.sdkwork.webserver.backend.sdk

data class TrafficUsageStatisticsResponse(
    val dateFrom: String? = null,
    val dateTo: String? = null,
    val platformScope: Boolean? = null,
    val totals: List<TrafficUsageTotal>? = null,
    val daily: List<TrafficUsageDailyPoint>? = null,
    val apps: List<TrafficUsageAppTotal>? = null,
    val tenants: List<TrafficUsageTenantTotal>? = null
)
