package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class ClusterHeartbeatSampleResponse {
    private String id;
    private Integer status;
    private Integer latencyMs;
    private Map<String, Object> metrics;
    private String reportedAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public Integer getLatencyMs() {
        return this.latencyMs;
    }

    public void setLatencyMs(Integer latencyMs) {
        this.latencyMs = latencyMs;
    }

    public Map<String, Object> getMetrics() {
        return this.metrics;
    }

    public void setMetrics(Map<String, Object> metrics) {
        this.metrics = metrics;
    }

    public String getReportedAt() {
        return this.reportedAt;
    }

    public void setReportedAt(String reportedAt) {
        this.reportedAt = reportedAt;
    }
}
