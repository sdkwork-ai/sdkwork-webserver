package com.sdkwork.webserver.app.sdk.model;


public class CreateListenerCertificateBindingRequest {
    private String certificateId;
    private String certificateVersionId;
    private Integer priority;
    private Boolean isDefault;

    public String getCertificateId() {
        return this.certificateId;
    }

    public void setCertificateId(String certificateId) {
        this.certificateId = certificateId;
    }

    public String getCertificateVersionId() {
        return this.certificateVersionId;
    }

    public void setCertificateVersionId(String certificateVersionId) {
        this.certificateVersionId = certificateVersionId;
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
}
