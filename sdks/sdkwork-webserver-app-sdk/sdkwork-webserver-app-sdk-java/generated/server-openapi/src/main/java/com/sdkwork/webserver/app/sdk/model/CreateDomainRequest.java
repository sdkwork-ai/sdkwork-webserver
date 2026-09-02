package com.sdkwork.webserver.app.sdk.model;


public class CreateDomainRequest {
    private String hostname;
    private Boolean isPrimary;
    private Boolean sslEnabled;
    private String sslProvider;

    public String getHostname() {
        return this.hostname;
    }

    public void setHostname(String hostname) {
        this.hostname = hostname;
    }

    public Boolean getIsPrimary() {
        return this.isPrimary;
    }

    public void setIsPrimary(Boolean isPrimary) {
        this.isPrimary = isPrimary;
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
}
