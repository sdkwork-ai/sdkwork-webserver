package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class IssueCertificateRequest {
    private List<String> domainIds;
    private Integer certType;
    private String keyAlgorithm;
    private Boolean autoRenew;

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
}
