package com.sdkwork.webserver.backend.sdk.model;


public class CertificateDistributionResponse {
    private String serverId;
    private String serverName;
    private String host;
    private String desiredSyncVersion;
    private String appliedSyncVersion;
    private String status;
    private String lastHeartbeatAt;

    public String getServerId() {
        return this.serverId;
    }

    public void setServerId(String serverId) {
        this.serverId = serverId;
    }

    public String getServerName() {
        return this.serverName;
    }

    public void setServerName(String serverName) {
        this.serverName = serverName;
    }

    public String getHost() {
        return this.host;
    }

    public void setHost(String host) {
        this.host = host;
    }

    public String getDesiredSyncVersion() {
        return this.desiredSyncVersion;
    }

    public void setDesiredSyncVersion(String desiredSyncVersion) {
        this.desiredSyncVersion = desiredSyncVersion;
    }

    public String getAppliedSyncVersion() {
        return this.appliedSyncVersion;
    }

    public void setAppliedSyncVersion(String appliedSyncVersion) {
        this.appliedSyncVersion = appliedSyncVersion;
    }

    public String getStatus() {
        return this.status;
    }

    public void setStatus(String status) {
        this.status = status;
    }

    public String getLastHeartbeatAt() {
        return this.lastHeartbeatAt;
    }

    public void setLastHeartbeatAt(String lastHeartbeatAt) {
        this.lastHeartbeatAt = lastHeartbeatAt;
    }
}
