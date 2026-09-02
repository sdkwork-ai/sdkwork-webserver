package com.sdkwork.webserver.backend.sdk.model;


public class ApplicationDomainResponse {
    private String id;
    private String hostname;
    private String rootDomainId;
    private String recordName;
    private String applicationId;
    private String applicationName;
    private String certificateCount;
    private Boolean isPrimary;
    private Boolean isVerified;
    private Boolean sslEnabled;
    private String sslProvider;
    private Integer status;
    private DomainDeploymentResponse latestDeployment;
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

    public String getRootDomainId() {
        return this.rootDomainId;
    }

    public void setRootDomainId(String rootDomainId) {
        this.rootDomainId = rootDomainId;
    }

    public String getRecordName() {
        return this.recordName;
    }

    public void setRecordName(String recordName) {
        this.recordName = recordName;
    }

    public String getApplicationId() {
        return this.applicationId;
    }

    public void setApplicationId(String applicationId) {
        this.applicationId = applicationId;
    }

    public String getApplicationName() {
        return this.applicationName;
    }

    public void setApplicationName(String applicationName) {
        this.applicationName = applicationName;
    }

    public String getCertificateCount() {
        return this.certificateCount;
    }

    public void setCertificateCount(String certificateCount) {
        this.certificateCount = certificateCount;
    }

    public Boolean getIsPrimary() {
        return this.isPrimary;
    }

    public void setIsPrimary(Boolean isPrimary) {
        this.isPrimary = isPrimary;
    }

    public Boolean getIsVerified() {
        return this.isVerified;
    }

    public void setIsVerified(Boolean isVerified) {
        this.isVerified = isVerified;
    }

    public Boolean getSslEnabled() {
        return this.sslEnabled;
    }

    public void setSslEnabled(Boolean sslEnabled) {
        this.sslEnabled = sslEnabled;
    }

    public String getSslProvider() {
        return this.sslProvider;
    }

    public void setSslProvider(String sslProvider) {
        this.sslProvider = sslProvider;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public DomainDeploymentResponse getLatestDeployment() {
        return this.latestDeployment;
    }

    public void setLatestDeployment(DomainDeploymentResponse latestDeployment) {
        this.latestDeployment = latestDeployment;
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
