package com.sdkwork.webserver.backend.sdk.model;


public class ServerResponse {
    private String id;
    private String name;
    private String host;
    private String tenantScopeHash;
    private Integer sshPort;
    private Integer status;
    private String lastHeartbeatAt;
    private String createdAt;

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

    public String getHost() {
        return this.host;
    }

    public void setHost(String host) {
        this.host = host;
    }

    public String getTenantScopeHash() {
        return this.tenantScopeHash;
    }

    public void setTenantScopeHash(String tenantScopeHash) {
        this.tenantScopeHash = tenantScopeHash;
    }

    public Integer getSshPort() {
        return this.sshPort;
    }

    public void setSshPort(Integer sshPort) {
        this.sshPort = sshPort;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getLastHeartbeatAt() {
        return this.lastHeartbeatAt;
    }

    public void setLastHeartbeatAt(String lastHeartbeatAt) {
        this.lastHeartbeatAt = lastHeartbeatAt;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }
}
