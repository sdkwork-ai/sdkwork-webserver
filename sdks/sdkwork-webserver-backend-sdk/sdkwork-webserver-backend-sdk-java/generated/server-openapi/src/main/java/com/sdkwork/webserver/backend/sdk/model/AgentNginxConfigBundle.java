package com.sdkwork.webserver.backend.sdk.model;


public class AgentNginxConfigBundle {
    private String configId;
    private String domain;
    private String configContent;
    private String fingerprint;
    private String version;

    public String getConfigId() {
        return this.configId;
    }

    public void setConfigId(String configId) {
        this.configId = configId;
    }

    public String getDomain() {
        return this.domain;
    }

    public void setDomain(String domain) {
        this.domain = domain;
    }

    public String getConfigContent() {
        return this.configContent;
    }

    public void setConfigContent(String configContent) {
        this.configContent = configContent;
    }

    public String getFingerprint() {
        return this.fingerprint;
    }

    public void setFingerprint(String fingerprint) {
        this.fingerprint = fingerprint;
    }

    public String getVersion() {
        return this.version;
    }

    public void setVersion(String version) {
        this.version = version;
    }
}
