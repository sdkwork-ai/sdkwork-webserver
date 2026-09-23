package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class ClusterSyncManifest {
    private String clusterId;
    private String kind;
    private String revision;
    private String sha256;
    private Map<String, Object> payload;
    private String createdAt;

    public String getClusterId() {
        return this.clusterId;
    }

    public void setClusterId(String clusterId) {
        this.clusterId = clusterId;
    }

    public String getKind() {
        return this.kind;
    }

    public void setKind(String kind) {
        this.kind = kind;
    }

    public String getRevision() {
        return this.revision;
    }

    public void setRevision(String revision) {
        this.revision = revision;
    }

    public String getSha256() {
        return this.sha256;
    }

    public void setSha256(String sha256) {
        this.sha256 = sha256;
    }

    public Map<String, Object> getPayload() {
        return this.payload;
    }

    public void setPayload(Map<String, Object> payload) {
        this.payload = payload;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }
}
