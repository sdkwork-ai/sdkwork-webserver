package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class IssueCertificateRequest {
    private List<String> domainIds;
    private Integer certType;
    private String keyAlgorithm;
    private Boolean autoRenew;
    private String certName;
    private String certificateScope;
    private String validationMethod;
    private Integer renewBeforeDays;
    private String caProfile;
    private String providerAccountId;

    public List<String> getDomainIds() {
        return this.domainIds;
    }

    public void setDomainIds(List<String> domainIds) {
        this.domainIds = domainIds;
    }

    public Integer getCertType() {
        return this.certType;
    }

    public void setCertType(Integer certType) {
        this.certType = certType;
    }

    public String getKeyAlgorithm() {
        return this.keyAlgorithm;
    }

    public void setKeyAlgorithm(String keyAlgorithm) {
        this.keyAlgorithm = keyAlgorithm;
    }

    public Boolean getAutoRenew() {
        return this.autoRenew;
    }

    public void setAutoRenew(Boolean autoRenew) {
        this.autoRenew = autoRenew;
    }

    public String getCertName() {
        return this.certName;
    }

    public void setCertName(String certName) {
        this.certName = certName;
    }

    public String getCertificateScope() {
        return this.certificateScope;
    }

    public void setCertificateScope(String certificateScope) {
        this.certificateScope = certificateScope;
    }

    public String getValidationMethod() {
        return this.validationMethod;
    }

    public void setValidationMethod(String validationMethod) {
        this.validationMethod = validationMethod;
    }

    public Integer getRenewBeforeDays() {
        return this.renewBeforeDays;
    }

    public void setRenewBeforeDays(Integer renewBeforeDays) {
        this.renewBeforeDays = renewBeforeDays;
    }

    public String getCaProfile() {
        return this.caProfile;
    }

    public void setCaProfile(String caProfile) {
        this.caProfile = caProfile;
    }

    public String getProviderAccountId() {
        return this.providerAccountId;
    }

    public void setProviderAccountId(String providerAccountId) {
        this.providerAccountId = providerAccountId;
    }
}
