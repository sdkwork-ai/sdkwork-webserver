package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class AgentHeartbeatRequest {
    private String agentVersion;
    private Boolean nginxEnabled;
    private String activeConfigs;
    private String lastSyncVersion;
    private List<AgentCertificateObservation> certificateObservations;

    public String getAgentVersion() {
        return this.agentVersion;
    }

    public void setAgentVersion(String agentVersion) {
        this.agentVersion = agentVersion;
    }

    public Boolean getNginxEnabled() {
        return this.nginxEnabled;
    }

    public void setNginxEnabled(Boolean nginxEnabled) {
        this.nginxEnabled = nginxEnabled;
    }

    public String getActiveConfigs() {
        return this.activeConfigs;
    }

    public void setActiveConfigs(String activeConfigs) {
        this.activeConfigs = activeConfigs;
    }

    public String getLastSyncVersion() {
        return this.lastSyncVersion;
    }

    public void setLastSyncVersion(String lastSyncVersion) {
        this.lastSyncVersion = lastSyncVersion;
    }

    public List<AgentCertificateObservation> getCertificateObservations() {
        return this.certificateObservations;
    }

    public void setCertificateObservations(List<AgentCertificateObservation> certificateObservations) {
        this.certificateObservations = certificateObservations;
    }
}
