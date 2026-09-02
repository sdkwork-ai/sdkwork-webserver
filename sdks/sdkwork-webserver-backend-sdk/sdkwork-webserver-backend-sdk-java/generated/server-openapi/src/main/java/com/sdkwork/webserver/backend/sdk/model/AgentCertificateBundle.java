package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class AgentCertificateBundle {
    private String certificateId;
    private String certName;
    private String fingerprint;
    private List<String> hostnames;
    private String fullchainPem;
    private String privkeyPem;

    public String getCertificateId() {
        return this.certificateId;
    }

    public void setCertificateId(String certificateId) {
        this.certificateId = certificateId;
    }

    public String getCertName() {
        return this.certName;
    }

    public void setCertName(String certName) {
        this.certName = certName;
    }

    public String getFingerprint() {
        return this.fingerprint;
    }

    public void setFingerprint(String fingerprint) {
        this.fingerprint = fingerprint;
    }

    public List<String> getHostnames() {
        return this.hostnames;
    }

    public void setHostnames(List<String> hostnames) {
        this.hostnames = hostnames;
    }

    public String getFullchainPem() {
        return this.fullchainPem;
    }

    public void setFullchainPem(String fullchainPem) {
        this.fullchainPem = fullchainPem;
    }

    public String getPrivkeyPem() {
        return this.privkeyPem;
    }

    public void setPrivkeyPem(String privkeyPem) {
        this.privkeyPem = privkeyPem;
    }
}
