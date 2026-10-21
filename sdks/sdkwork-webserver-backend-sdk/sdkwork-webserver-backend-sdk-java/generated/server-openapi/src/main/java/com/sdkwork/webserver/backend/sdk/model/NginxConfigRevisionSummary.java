package com.sdkwork.webserver.backend.sdk.model;


public class NginxConfigRevisionSummary {
    private Integer versionNo;
    private String configName;
    private String configHash;
    private String createdBy;
    private String createdAt;

    public Integer getVersionNo() {
        return this.versionNo;
    }

    public void setVersionNo(Integer versionNo) {
        this.versionNo = versionNo;
    }

    public String getConfigName() {
        return this.configName;
    }

    public void setConfigName(String configName) {
        this.configName = configName;
    }

    public String getConfigHash() {
        return this.configHash;
    }

    public void setConfigHash(String configHash) {
        this.configHash = configHash;
    }

    public String getCreatedBy() {
        return this.createdBy;
    }

    public void setCreatedBy(String createdBy) {
        this.createdBy = createdBy;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }
}
