package com.sdkwork.webserver.backend.sdk.model;


public class CreateServerRequest {
    private String name;
    private String host;
    private String tenantScopeHash;
    private Integer sshPort;

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
}
