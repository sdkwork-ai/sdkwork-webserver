package com.sdkwork.webserver.backend.sdk.model;


public class CreateApplicationDeploymentRequest {
    private String sourceVersionId;
    private Integer deployType;
    private String environment;
    private String versionTag;
    private String commitHash;
    private String sourceRef;
    private String artifactDriveUri;
    private String artifactSize;
    private String artifactHash;

    public String getSourceVersionId() {
        return this.sourceVersionId;
    }

    public void setSourceVersionId(String sourceVersionId) {
        this.sourceVersionId = sourceVersionId;
    }

    public Integer getDeployType() {
        return this.deployType;
    }

    public void setDeployType(Integer deployType) {
        this.deployType = deployType;
    }

    public String getEnvironment() {
        return this.environment;
    }

    public void setEnvironment(String environment) {
        this.environment = environment;
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
}
