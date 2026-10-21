package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class CreatePluginRequest {
    private String pluginKey;
    private String displayName;
    private String summary;
    private String version;
    private String categoryId;
    private List<String> supportedHostTools;
    private List<String> contributedCapabilities;
    private String sourceKind;
    private String gitRepository;
    private String gitRef;
    private String artifactRef;
    private String checksumSha256;
    private String archiveFileName;
    private String status;

    public String getPluginKey() {
        return this.pluginKey;
    }

    public void setPluginKey(String pluginKey) {
        this.pluginKey = pluginKey;
    }

    public String getDisplayName() {
        return this.displayName;
    }

    public void setDisplayName(String displayName) {
        this.displayName = displayName;
    }

    public String getSummary() {
        return this.summary;
    }

    public void setSummary(String summary) {
        this.summary = summary;
    }

    public String getVersion() {
        return this.version;
    }

    public void setVersion(String version) {
        this.version = version;
    }

    public String getCategoryId() {
        return this.categoryId;
    }

    public void setCategoryId(String categoryId) {
        this.categoryId = categoryId;
    }

    public List<String> getSupportedHostTools() {
        return this.supportedHostTools;
    }

    public void setSupportedHostTools(List<String> supportedHostTools) {
        this.supportedHostTools = supportedHostTools;
    }

    public List<String> getContributedCapabilities() {
        return this.contributedCapabilities;
    }

    public void setContributedCapabilities(List<String> contributedCapabilities) {
        this.contributedCapabilities = contributedCapabilities;
    }

    public String getSourceKind() {
        return this.sourceKind;
    }

    public void setSourceKind(String sourceKind) {
        this.sourceKind = sourceKind;
    }

    public String getGitRepository() {
        return this.gitRepository;
    }

    public void setGitRepository(String gitRepository) {
        this.gitRepository = gitRepository;
    }

    public String getGitRef() {
        return this.gitRef;
    }

    public void setGitRef(String gitRef) {
        this.gitRef = gitRef;
    }

    public String getArtifactRef() {
        return this.artifactRef;
    }

    public void setArtifactRef(String artifactRef) {
        this.artifactRef = artifactRef;
    }

    public String getChecksumSha256() {
        return this.checksumSha256;
    }

    public void setChecksumSha256(String checksumSha256) {
        this.checksumSha256 = checksumSha256;
    }

    public String getArchiveFileName() {
        return this.archiveFileName;
    }

    public void setArchiveFileName(String archiveFileName) {
        this.archiveFileName = archiveFileName;
    }

    public String getStatus() {
        return this.status;
    }

    public void setStatus(String status) {
        this.status = status;
    }
}
