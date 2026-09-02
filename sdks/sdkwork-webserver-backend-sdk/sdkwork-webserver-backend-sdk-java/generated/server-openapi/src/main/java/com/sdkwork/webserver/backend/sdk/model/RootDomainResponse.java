package com.sdkwork.webserver.backend.sdk.model;


public class RootDomainResponse {
    private String id;
    private String hostname;
    private Integer status;
    private String subdomainCount;
    private String boundSubdomainCount;
    private String verifiedSubdomainCount;
    private String httpsSubdomainCount;
    private String activeDeploymentCount;
    private String createdAt;
    private String updatedAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getHostname() {
        return this.hostname;
    }

    public void setHostname(String hostname) {
        this.hostname = hostname;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getSubdomainCount() {
        return this.subdomainCount;
    }

    public void setSubdomainCount(String subdomainCount) {
        this.subdomainCount = subdomainCount;
    }

    public String getBoundSubdomainCount() {
        return this.boundSubdomainCount;
    }

    public void setBoundSubdomainCount(String boundSubdomainCount) {
        this.boundSubdomainCount = boundSubdomainCount;
    }

    public String getVerifiedSubdomainCount() {
        return this.verifiedSubdomainCount;
    }

    public void setVerifiedSubdomainCount(String verifiedSubdomainCount) {
        this.verifiedSubdomainCount = verifiedSubdomainCount;
    }

    public String getHttpsSubdomainCount() {
        return this.httpsSubdomainCount;
    }

    public void setHttpsSubdomainCount(String httpsSubdomainCount) {
        this.httpsSubdomainCount = httpsSubdomainCount;
    }

    public String getActiveDeploymentCount() {
        return this.activeDeploymentCount;
    }

    public void setActiveDeploymentCount(String activeDeploymentCount) {
        this.activeDeploymentCount = activeDeploymentCount;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }

    public String getUpdatedAt() {
        return this.updatedAt;
    }

    public void setUpdatedAt(String updatedAt) {
        this.updatedAt = updatedAt;
    }
}
