package com.sdkwork.webserver.backend.sdk.model;


public class UpdateDomainApplicationBindingRequest {
    private String applicationId;
    private Boolean isPrimary;

    public String getApplicationId() {
        return this.applicationId;
    }

    public void setApplicationId(String applicationId) {
        this.applicationId = applicationId;
    }

    public Boolean getIsPrimary() {
        return this.isPrimary;
    }

    public void setIsPrimary(Boolean isPrimary) {
        this.isPrimary = isPrimary;
    }
}
