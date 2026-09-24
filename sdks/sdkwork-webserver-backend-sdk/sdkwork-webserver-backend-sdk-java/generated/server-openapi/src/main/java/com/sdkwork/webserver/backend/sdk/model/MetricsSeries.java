package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class MetricsSeries {
    private String metric;
    private String unit;
    private List<MetricsSeriesPoint> points;

    public String getMetric() {
        return this.metric;
    }

    public void setMetric(String metric) {
        this.metric = metric;
    }

    public String getUnit() {
        return this.unit;
    }

    public void setUnit(String unit) {
        this.unit = unit;
    }

    public List<MetricsSeriesPoint> getPoints() {
        return this.points;
    }

    public void setPoints(List<MetricsSeriesPoint> points) {
        this.points = points;
    }
}
