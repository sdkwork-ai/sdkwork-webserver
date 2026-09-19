package com.sdkwork.webserver.backend.sdk.model;


public class ClusterResponse {
    private String id;
    private String name;
    private String code;
    private String description;
    private Integer status;
    private Integer heartbeatIntervalSeconds;
    private Integer offlineThresholdSeconds;
    private String hostCount;
    private String instanceCount;
    private String onlineInstanceCount;
    private String createdAt;
    private String updatedAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

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

    public String getHostCount() {
        return this.hostCount;
    }

    public void setHostCount(String hostCount) {
        this.hostCount = hostCount;
    }

    public String getInstanceCount() {
        return this.instanceCount;
    }

    public void setInstanceCount(String instanceCount) {
        this.instanceCount = instanceCount;
    }

    public String getOnlineInstanceCount() {
        return this.onlineInstanceCount;
    }

    public void setOnlineInstanceCount(String onlineInstanceCount) {
        this.onlineInstanceCount = onlineInstanceCount;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }

    public String getUpdatedAt() {
        return this.updatedAt;
    }

    public void setUpdatedAt(String updatedAt) {
        this.updatedAt = updatedAt;
    }
}
