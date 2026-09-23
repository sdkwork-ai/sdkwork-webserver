package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class UpdateClusterRequest {
    private String name;
    private String description;
    private Integer status;
    private Integer heartbeatIntervalSeconds;
    private Integer offlineThresholdSeconds;
    private String lbStrategy;
    private List<String> servedDomains;

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public String getDescription() {
        return this.description;
    }

    public void setDescription(String description) {
        this.description = description;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public Integer getHeartbeatIntervalSeconds() {
        return this.heartbeatIntervalSeconds;
    }

    public void setHeartbeatIntervalSeconds(Integer heartbeatIntervalSeconds) {
        this.heartbeatIntervalSeconds = heartbeatIntervalSeconds;
    }

    public Integer getOfflineThresholdSeconds() {
        return this.offlineThresholdSeconds;
    }

    public void setOfflineThresholdSeconds(Integer offlineThresholdSeconds) {
        this.offlineThresholdSeconds = offlineThresholdSeconds;
    }

    public String getLbStrategy() {
        return this.lbStrategy;
    }

    public void setLbStrategy(String lbStrategy) {
        this.lbStrategy = lbStrategy;
    }

    public List<String> getServedDomains() {
        return this.servedDomains;
    }

    public void setServedDomains(List<String> servedDomains) {
        this.servedDomains = servedDomains;
    }
}
