package com.sdkwork.webserver.backend.sdk.model;


public class ClusterProbeRunResponse {
    private Boolean healthy;
    private Integer latencyMs;
    private Integer failures;
    private Boolean ejected;
    private Boolean recovered;

    public Boolean getHealthy() {
        return this.healthy;
    }

    public void setHealthy(Boolean healthy) {
        this.healthy = healthy;
    }

    public Integer getLatencyMs() {
        return this.latencyMs;
    }

    public void setLatencyMs(Integer latencyMs) {
        this.latencyMs = latencyMs;
    }

    public Integer getFailures() {
        return this.failures;
    }

    public void setFailures(Integer failures) {
        this.failures = failures;
    }

    public Boolean getEjected() {
        return this.ejected;
    }

    public void setEjected(Boolean ejected) {
        this.ejected = ejected;
    }

    public Boolean getRecovered() {
        return this.recovered;
    }

    public void setRecovered(Boolean recovered) {
        this.recovered = recovered;
    }
}
