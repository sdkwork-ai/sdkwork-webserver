package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class TrafficUsageStatisticsResponse {
    private String dateFrom;
    private String dateTo;
    private Boolean platformScope;
    private List<TrafficUsageTotal> totals;
    private List<TrafficUsageDailyPoint> daily;
    private List<TrafficUsageAppTotal> apps;
    private List<TrafficUsageTenantTotal> tenants;

    public String getDateFrom() {
        return this.dateFrom;
    }

    public void setDateFrom(String dateFrom) {
        this.dateFrom = dateFrom;
    }

    public String getDateTo() {
        return this.dateTo;
    }

    public void setDateTo(String dateTo) {
        this.dateTo = dateTo;
    }

    public Boolean getPlatformScope() {
        return this.platformScope;
    }

    public void setPlatformScope(Boolean platformScope) {
        this.platformScope = platformScope;
    }

    public List<TrafficUsageTotal> getTotals() {
        return this.totals;
    }

    public void setTotals(List<TrafficUsageTotal> totals) {
        this.totals = totals;
    }

    public List<TrafficUsageDailyPoint> getDaily() {
        return this.daily;
    }

    public void setDaily(List<TrafficUsageDailyPoint> daily) {
        this.daily = daily;
    }

    public List<TrafficUsageAppTotal> getApps() {
        return this.apps;
    }

    public void setApps(List<TrafficUsageAppTotal> apps) {
        this.apps = apps;
    }

    public List<TrafficUsageTenantTotal> getTenants() {
        return this.tenants;
    }

    public void setTenants(List<TrafficUsageTenantTotal> tenants) {
        this.tenants = tenants;
    }
}
