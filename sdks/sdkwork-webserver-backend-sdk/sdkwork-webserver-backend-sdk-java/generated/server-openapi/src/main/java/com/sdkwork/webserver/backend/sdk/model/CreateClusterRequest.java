package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class CreateClusterRequest {
    private String name;
    private String code;
    private String description;
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

    public String getCode() {
        return this.code;
    }

    public void setCode(String code) {
        this.code = code;
    }

    public String getDescription() {
        return this.description;
    }

    public void setDescription(String description) {
        this.description = description;
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
