package com.sdkwork.webserver.backend.sdk.model;


public class CreateRootDomainRequest {
    private String hostname;
    private String cloudAccountId;

    public String getHostname() {
        return this.hostname;
    }

    public void setHostname(String hostname) {
        this.hostname = hostname;
    }

    public String getCloudAccountId() {
        return this.cloudAccountId;
    }

    public void setCloudAccountId(String cloudAccountId) {
        this.cloudAccountId = cloudAccountId;
    }
}
