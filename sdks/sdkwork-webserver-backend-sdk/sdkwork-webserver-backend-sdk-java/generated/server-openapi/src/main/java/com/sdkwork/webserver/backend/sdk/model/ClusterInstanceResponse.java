package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class ClusterInstanceResponse {
    private String id;
    private String clusterId;
    private String hostId;
    private String hostName;
    private String name;
    private String role;
    private String environment;
    private Integer processPid;
    private String processStartedAt;
    private String bindHost;
    private Integer bindPort;
    private String publicEndpoint;
    private String buildVersion;
    private Integer status;
    private String healthState;
    private String lastHeartbeatAt;
    private String lastOnlineAt;
    private String uptimeSeconds;
    private Map<String, Object> metrics;
    private String createdAt;
    private String updatedAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getClusterId() {
        return this.clusterId;
    }

    public void setClusterId(String clusterId) {
        this.clusterId = clusterId;
    }

    public String getHostId() {
        return this.hostId;
    }

    public void setHostId(String hostId) {
        this.hostId = hostId;
    }

    public String getHostName() {
        return this.hostName;
    }

    public void setHostName(String hostName) {
        this.hostName = hostName;
    }

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public String getRole() {
        return this.role;
    }

    public void setRole(String role) {
        this.role = role;
    }

    public String getEnvironment() {
        return this.environment;
    }

    public void setEnvironment(String environment) {
        this.environment = environment;
    }

    public Integer getProcessPid() {
        return this.processPid;
    }

    public void setProcessPid(Integer processPid) {
        this.processPid = processPid;
    }

    public String getProcessStartedAt() {
        return this.processStartedAt;
    }

    public void setProcessStartedAt(String processStartedAt) {
        this.processStartedAt = processStartedAt;
    }

    public String getBindHost() {
        return this.bindHost;
    }

    public void setBindHost(String bindHost) {
        this.bindHost = bindHost;
    }

    public Integer getBindPort() {
        return this.bindPort;
    }

    public void setBindPort(Integer bindPort) {
        this.bindPort = bindPort;
    }

    public String getPublicEndpoint() {
        return this.publicEndpoint;
    }

    public void setPublicEndpoint(String publicEndpoint) {
        this.publicEndpoint = publicEndpoint;
    }

    public String getBuildVersion() {
        return this.buildVersion;
    }

    public void setBuildVersion(String buildVersion) {
        this.buildVersion = buildVersion;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getHealthState() {
        return this.healthState;
    }

    public void setHealthState(String healthState) {
        this.healthState = healthState;
    }

    public String getLastHeartbeatAt() {
        return this.lastHeartbeatAt;
    }

    public void setLastHeartbeatAt(String lastHeartbeatAt) {
        this.lastHeartbeatAt = lastHeartbeatAt;
    }

    public String getLastOnlineAt() {
        return this.lastOnlineAt;
    }

    public void setLastOnlineAt(String lastOnlineAt) {
        this.lastOnlineAt = lastOnlineAt;
    }

    public String getUptimeSeconds() {
        return this.uptimeSeconds;
    }

    public void setUptimeSeconds(String uptimeSeconds) {
        this.uptimeSeconds = uptimeSeconds;
    }

    public Map<String, Object> getMetrics() {
        return this.metrics;
    }

    public void setMetrics(Map<String, Object> metrics) {
        this.metrics = metrics;
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
