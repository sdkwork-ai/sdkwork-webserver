package com.sdkwork.web.app.sdk.model;


public class ListenerCertificateBindingResponse {
    private String id;
    private String applicationId;
    private String domainId;
    private String certificateId;
    private String desiredCertificateVersionId;
    private String currentCertificateVersionId;
    private ListenerCertificateSummaryResponse desiredCertificate;
    private ListenerCertificateSummaryResponse currentCertificate;
    private String keyAlgorithm;
    private Integer priority;
    private Boolean isDefault;
    private String status;
    private String activatedAt;
    private String createdAt;
    private String updatedAt;

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

    public String getDomainId() {
        return this.domainId;
    }

    public void setDomainId(String domainId) {
        this.domainId = domainId;
    }

    public String getCertificateId() {
        return this.certificateId;
    }

    public void setCertificateId(String certificateId) {
        this.certificateId = certificateId;
    }

    public String getDesiredCertificateVersionId() {
        return this.desiredCertificateVersionId;
    }

    public void setDesiredCertificateVersionId(String desiredCertificateVersionId) {
        this.desiredCertificateVersionId = desiredCertificateVersionId;
    }

    public String getCurrentCertificateVersionId() {
        return this.currentCertificateVersionId;
    }

    public void setCurrentCertificateVersionId(String currentCertificateVersionId) {
        this.currentCertificateVersionId = currentCertificateVersionId;
    }

    public ListenerCertificateSummaryResponse getDesiredCertificate() {
        return this.desiredCertificate;
    }

    public void setDesiredCertificate(ListenerCertificateSummaryResponse desiredCertificate) {
        this.desiredCertificate = desiredCertificate;
    }

    public ListenerCertificateSummaryResponse getCurrentCertificate() {
        return this.currentCertificate;
    }

    public void setCurrentCertificate(ListenerCertificateSummaryResponse currentCertificate) {
        this.currentCertificate = currentCertificate;
    }

    public String getKeyAlgorithm() {
        return this.keyAlgorithm;
    }

    public void setKeyAlgorithm(String keyAlgorithm) {
        this.keyAlgorithm = keyAlgorithm;
    }

    public Integer getPriority() {
        return this.priority;
    }

    public void setPriority(Integer priority) {
        this.priority = priority;
    }

    public Boolean getIsDefault() {
        return this.isDefault;
    }

    public void setIsDefault(Boolean isDefault) {
        this.isDefault = isDefault;
    }

    public String getStatus() {
        return this.status;
    }

    public void setStatus(String status) {
        this.status = status;
    }

    public String getActivatedAt() {
        return this.activatedAt;
    }

    public void setActivatedAt(String activatedAt) {
        this.activatedAt = activatedAt;
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
