package com.sdkwork.webserver.backend.sdk.model;


public class AgentHeartbeatResponse {
    private String serverId;
    private Integer status;
    private String acknowledgedAt;

    public String getServerId() {
        return this.serverId;
    }

    public void setServerId(String serverId) {
        this.serverId = serverId;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getAcknowledgedAt() {
        return this.acknowledgedAt;
    }

    public void setAcknowledgedAt(String acknowledgedAt) {
        this.acknowledgedAt = acknowledgedAt;
    }
}
