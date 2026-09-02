package com.sdkwork.webserver.backend.sdk.model;


public class UpdateCertificateRequest {
    private Boolean autoRenew;

    public Boolean getAutoRenew() {
        return this.autoRenew;
    }

    public void setAutoRenew(Boolean autoRenew) {
        this.autoRenew = autoRenew;
    }
}
