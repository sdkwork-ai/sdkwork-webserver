package com.sdkwork.webserver.backend.sdk.model;


public class ImportApplicationGitSourceVersionRequest {
    private String versionTag;
    private String repositoryUrl;
    private String gitRef;

    public String getVersionTag() {
        return this.versionTag;
    }

    public void setVersionTag(String versionTag) {
        this.versionTag = versionTag;
    }

    public String getRepositoryUrl() {
        return this.repositoryUrl;
    }

    public void setRepositoryUrl(String repositoryUrl) {
        this.repositoryUrl = repositoryUrl;
    }

    public String getGitRef() {
        return this.gitRef;
    }

    public void setGitRef(String gitRef) {
        this.gitRef = gitRef;
    }
}
