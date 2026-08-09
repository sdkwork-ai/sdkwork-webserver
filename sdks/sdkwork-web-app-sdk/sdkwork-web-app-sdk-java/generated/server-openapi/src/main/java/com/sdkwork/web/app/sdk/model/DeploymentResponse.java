package com.sdkwork.web.app.sdk.model;


public class DeploymentResponse {
    private String id;
    private String applicationId;
    private Integer deployType;
    private String sourceVersionId;
    private String versionTag;
    private String commitHash;
    private String sourceRef;
    private String rollbackFromDeploymentId;
    private String environment;
    private String artifactDriveUri;
    private String artifactSize;
    private String artifactHash;
    private Integer status;
    private String startedAt;
    private String completedAt;
    private String durationMs;
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

    public Integer getDeployType() {
        return this.deployType;
    }

    public void setDeployType(Integer deployType) {
        this.deployType = deployType;
    }

    public String getSourceVersionId() {
        return this.sourceVersionId;
    }

    public void setSourceVersionId(String sourceVersionId) {
        this.sourceVersionId = sourceVersionId;
    }

    public String getVersionTag() {
        return this.versionTag;
    }

    public void setVersionTag(String versionTag) {
        this.versionTag = versionTag;
    }

    public String getCommitHash() {
        return this.commitHash;
    }

    public void setCommitHash(String commitHash) {
        this.commitHash = commitHash;
    }

    public String getSourceRef() {
        return this.sourceRef;
    }

    public void setSourceRef(String sourceRef) {
        this.sourceRef = sourceRef;
    }

    public String getRollbackFromDeploymentId() {
        return this.rollbackFromDeploymentId;
    }

    public void setRollbackFromDeploymentId(String rollbackFromDeploymentId) {
        this.rollbackFromDeploymentId = rollbackFromDeploymentId;
    }

    public String getEnvironment() {
        return this.environment;
    }

    public void setEnvironment(String environment) {
        this.environment = environment;
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

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getStartedAt() {
        return this.startedAt;
    }

    public void setStartedAt(String startedAt) {
        this.startedAt = startedAt;
    }

    public String getCompletedAt() {
        return this.completedAt;
    }

    public void setCompletedAt(String completedAt) {
        this.completedAt = completedAt;
    }

    public String getDurationMs() {
        return this.durationMs;
    }

    public void setDurationMs(String durationMs) {
        this.durationMs = durationMs;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }
}
