package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class MetricsMetricTotals {
    private String metric;
    private String unit;
    private List<MetricsWindowValue> values;

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

    public List<MetricsWindowValue> getValues() {
        return this.values;
    }

    public void setValues(List<MetricsWindowValue> values) {
        this.values = values;
    }
}
