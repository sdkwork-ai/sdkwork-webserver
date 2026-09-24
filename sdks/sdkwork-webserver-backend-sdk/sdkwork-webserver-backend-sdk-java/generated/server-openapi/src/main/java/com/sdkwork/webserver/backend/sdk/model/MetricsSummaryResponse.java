package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class MetricsSummaryResponse {
    private String asOf;
    private Boolean platformScope;
    private String trafficSince;
    private List<MetricsWindowBounds> windows;
    private List<MetricsMetricTotals> entities;
    private List<MetricsMetricTotals> traffic;
    private List<MetricsMetricTotals> storage;
    private List<MetricsSeries> series;
    private MetricsSeriesWindow seriesWindow;
    private List<String> unassembledMetrics;

    public String getAsOf() {
        return this.asOf;
    }

    public void setAsOf(String asOf) {
        this.asOf = asOf;
    }

    public Boolean getPlatformScope() {
        return this.platformScope;
    }

    public void setPlatformScope(Boolean platformScope) {
        this.platformScope = platformScope;
    }

    public String getTrafficSince() {
        return this.trafficSince;
    }

    public void setTrafficSince(String trafficSince) {
        this.trafficSince = trafficSince;
    }

    public List<MetricsWindowBounds> getWindows() {
        return this.windows;
    }

    public void setWindows(List<MetricsWindowBounds> windows) {
        this.windows = windows;
    }

    public List<MetricsMetricTotals> getEntities() {
        return this.entities;
    }

    public void setEntities(List<MetricsMetricTotals> entities) {
        this.entities = entities;
    }

    public List<MetricsMetricTotals> getTraffic() {
        return this.traffic;
    }

    public void setTraffic(List<MetricsMetricTotals> traffic) {
        this.traffic = traffic;
    }

    public List<MetricsMetricTotals> getStorage() {
        return this.storage;
    }

    public void setStorage(List<MetricsMetricTotals> storage) {
        this.storage = storage;
    }

    public List<MetricsSeries> getSeries() {
        return this.series;
    }

    public void setSeries(List<MetricsSeries> series) {
        this.series = series;
    }

    public MetricsSeriesWindow getSeriesWindow() {
        return this.seriesWindow;
    }

    public void setSeriesWindow(MetricsSeriesWindow seriesWindow) {
        this.seriesWindow = seriesWindow;
    }

    public List<String> getUnassembledMetrics() {
        return this.unassembledMetrics;
    }

    public void setUnassembledMetrics(List<String> unassembledMetrics) {
        this.unassembledMetrics = unassembledMetrics;
    }
}
