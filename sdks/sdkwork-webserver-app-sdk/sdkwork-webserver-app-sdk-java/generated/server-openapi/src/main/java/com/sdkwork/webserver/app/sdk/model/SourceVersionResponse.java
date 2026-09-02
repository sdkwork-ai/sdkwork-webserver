package com.sdkwork.webserver.app.sdk.model;


public class SourceVersionResponse {
    private String id;
    private String applicationId;
    private String versionTag;
    private String sourceType;
    private String sourceRef;
    private String commitHash;
    private String artifactDriveUri;
    private String artifactSize;
    private String artifactHash;
    private SourceVersionConfigSnapshot configSnapshot;
    private Integer status;
    private Boolean retained;
    private String createdAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getApplicationId() {
        return this.applicationId;
    }

    public void setApplicationId(String applicationId) {
        this.applicationId = applicationId;
    }

    public String getVersionTag() {
        return this.versionTag;
    }

    public void setVersionTag(String versionTag) {
        this.versionTag = versionTag;
    }

    public String getSourceType() {
        return this.sourceType;
    }

    public void setSourceType(String sourceType) {
        this.sourceType = sourceType;
    }

    public String getSourceRef() {
        return this.sourceRef;
    }

    public void setSourceRef(String sourceRef) {
        this.sourceRef = sourceRef;
    }

    public String getCommitHash() {
        return this.commitHash;
    }

    public void setCommitHash(String commitHash) {
        this.commitHash = commitHash;
    }

    public String getArtifactDriveUri() {
        return this.artifactDriveUri;
    }

    public void setArtifactDriveUri(String artifactDriveUri) {
        this.artifactDriveUri = artifactDriveUri;
    }

    public String getArtifactSize() {
        return this.artifactSize;
    }

    public void setArtifactSize(String artifactSize) {
        this.artifactSize = artifactSize;
    }

    public String getArtifactHash() {
        return this.artifactHash;
    }

    public void setArtifactHash(String artifactHash) {
        this.artifactHash = artifactHash;
    }

    public SourceVersionConfigSnapshot getConfigSnapshot() {
        return this.configSnapshot;
    }

    public void setConfigSnapshot(SourceVersionConfigSnapshot configSnapshot) {
        this.configSnapshot = configSnapshot;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public Boolean getRetained() {
        return this.retained;
    }

    public void setRetained(Boolean retained) {
        this.retained = retained;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }
}
