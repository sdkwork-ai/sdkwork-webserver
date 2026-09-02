package com.sdkwork.webserver.app.sdk.model;

import java.util.List;

public class ListenerCertificateSummaryResponse {
    private String certName;
    private List<CertificateIdentifierResponse> identifiers;
    private String issuer;
    private String fingerprint;
    private String notAfter;
    private String status;

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

    public String getNotAfter() {
        return this.notAfter;
    }

    public void setNotAfter(String notAfter) {
        this.notAfter = notAfter;
    }

    public String getStatus() {
        return this.status;
    }

    public void setStatus(String status) {
        this.status = status;
    }
}
