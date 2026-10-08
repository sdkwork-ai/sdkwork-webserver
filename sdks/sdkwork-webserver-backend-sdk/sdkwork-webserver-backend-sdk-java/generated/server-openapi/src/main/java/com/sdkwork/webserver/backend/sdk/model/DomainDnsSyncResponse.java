package com.sdkwork.webserver.backend.sdk.model;


public class DomainDnsSyncResponse {
    private String recordCount;
    private String syncedAt;
    private String zoneApex;
    private String dnsProvider;
    private String cloudAccountId;

    public String getRecordCount() {
        return this.recordCount;
    }

    public void setRecordCount(String recordCount) {
        this.recordCount = recordCount;
    }

    public String getSyncedAt() {
        return this.syncedAt;
    }

    public void setSyncedAt(String syncedAt) {
        this.syncedAt = syncedAt;
    }

    public String getZoneApex() {
        return this.zoneApex;
    }

    public void setZoneApex(String zoneApex) {
        this.zoneApex = zoneApex;
    }

    public String getDnsProvider() {
        return this.dnsProvider;
    }

    public void setDnsProvider(String dnsProvider) {
        this.dnsProvider = dnsProvider;
    }

    public String getCloudAccountId() {
        return this.cloudAccountId;
    }

    public void setCloudAccountId(String cloudAccountId) {
        this.cloudAccountId = cloudAccountId;
    }
}
