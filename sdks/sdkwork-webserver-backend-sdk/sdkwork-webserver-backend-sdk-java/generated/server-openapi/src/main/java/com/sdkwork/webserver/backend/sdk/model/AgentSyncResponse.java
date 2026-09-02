package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class AgentSyncResponse {
    private String serverId;
    private String syncVersion;
    private Boolean unchanged;
    private List<AgentNginxConfigBundle> nginxConfigs;
    private List<AgentCertificateBundle> certificates;

    public String getServerId() {
        return this.serverId;
    }

    public void setServerId(String serverId) {
        this.serverId = serverId;
    }

    public String getSyncVersion() {
        return this.syncVersion;
    }

    public void setSyncVersion(String syncVersion) {
        this.syncVersion = syncVersion;
    }

    public Boolean getUnchanged() {
        return this.unchanged;
    }

    public void setUnchanged(Boolean unchanged) {
        this.unchanged = unchanged;
    }

    public List<AgentNginxConfigBundle> getNginxConfigs() {
        return this.nginxConfigs;
    }

    public void setNginxConfigs(List<AgentNginxConfigBundle> nginxConfigs) {
        this.nginxConfigs = nginxConfigs;
    }

    public List<AgentCertificateBundle> getCertificates() {
        return this.certificates;
    }

    public void setCertificates(List<AgentCertificateBundle> certificates) {
        this.certificates = certificates;
    }
}
