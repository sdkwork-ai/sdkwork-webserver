package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class CertificateResponse {
    private String id;
    private String certName;
    private List<CertificateIdentifierResponse> identifiers;
    private Integer certType;
    private String issuer;
    private String fingerprint;
    private String keyAlgorithm;
    private String notBefore;
    private String notAfter;
    private Boolean autoRenew;
    private String renewalStatus;
    private String status;
    private String createdAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getCertName() {
        return this.certName;
    }

    public void setCertName(String certName) {
        this.certName = certName;
    }

    public List<CertificateIdentifierResponse> getIdentifiers() {
        return this.identifiers;
    }

    public void setIdentifiers(List<CertificateIdentifierResponse> identifiers) {
        this.identifiers = identifiers;
    }

    public Integer getCertType() {
        return this.certType;
    }

    public void setCertType(Integer certType) {
        this.certType = certType;
    }

    public String getIssuer() {
        return this.issuer;
    }

    public void setIssuer(String issuer) {
        this.issuer = issuer;
    }

    public String getFingerprint() {
        return this.fingerprint;
    }

    public void setFingerprint(String fingerprint) {
        this.fingerprint = fingerprint;
    }

    public String getKeyAlgorithm() {
        return this.keyAlgorithm;
    }

    public void setKeyAlgorithm(String keyAlgorithm) {
        this.keyAlgorithm = keyAlgorithm;
    }

    public String getNotBefore() {
        return this.notBefore;
    }

    public void setNotBefore(String notBefore) {
        this.notBefore = notBefore;
    }

    public String getNotAfter() {
        return this.notAfter;
    }

    public void setNotAfter(String notAfter) {
        this.notAfter = notAfter;
    }

    public Boolean getAutoRenew() {
        return this.autoRenew;
    }

    public void setAutoRenew(Boolean autoRenew) {
        this.autoRenew = autoRenew;
    }

    public String getRenewalStatus() {
        return this.renewalStatus;
    }

    public void setRenewalStatus(String renewalStatus) {
        this.renewalStatus = renewalStatus;
    }

    public String getStatus() {
        return this.status;
    }

    public void setStatus(String status) {
        this.status = status;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }
}
