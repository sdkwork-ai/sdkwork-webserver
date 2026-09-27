package com.sdkwork.webserver.backend.sdk.model;


public class DnsAccountResponse {
    private String accountId;
    private String provider;
    private String zoneApex;

    public String getAccountId() {
        return this.accountId;
    }

    public void setAccountId(String accountId) {
        this.accountId = accountId;
    }

    public String getProvider() {
        return this.provider;
    }

    public void setProvider(String provider) {
        this.provider = provider;
    }

    public String getZoneApex() {
        return this.zoneApex;
    }

    public void setZoneApex(String zoneApex) {
        this.zoneApex = zoneApex;
    }
}
