package com.sdkwork.webserver.backend.sdk.model;


public class UpdateRootDomainRequest {
    private String displayName;
    private String dnsProvider;
    private String providerZoneRef;
    private Integer status;

    public String getDisplayName() {
        return this.displayName;
    }

    public void setDisplayName(String displayName) {
        this.displayName = displayName;
    }

    public String getDnsProvider() {
        return this.dnsProvider;
    }

    public void setDnsProvider(String dnsProvider) {
        this.dnsProvider = dnsProvider;
    }

    public String getProviderZoneRef() {
        return this.providerZoneRef;
    }

    public void setProviderZoneRef(String providerZoneRef) {
        this.providerZoneRef = providerZoneRef;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }
}
