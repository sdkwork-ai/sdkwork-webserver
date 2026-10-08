package com.sdkwork.webserver.backend.sdk.model;


public class DomainDnsRecordResponse {
    private String id;
    private String recordName;
    private String host;
    private String recordType;
    private String recordValue;
    private Integer ttlSeconds;
    private Integer priority;
    private String recordLine;
    private String recordStatus;
    private String domainId;
    private String dnsProvider;
    private String cloudAccountId;
    private String providerRecordRef;
    private String syncedAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getRecordName() {
        return this.recordName;
    }

    public void setRecordName(String recordName) {
        this.recordName = recordName;
    }

    public String getHost() {
        return this.host;
    }

    public void setHost(String host) {
        this.host = host;
    }

    public String getRecordType() {
        return this.recordType;
    }

    public void setRecordType(String recordType) {
        this.recordType = recordType;
    }

    public String getRecordValue() {
        return this.recordValue;
    }

    public void setRecordValue(String recordValue) {
        this.recordValue = recordValue;
    }

    public Integer getTtlSeconds() {
        return this.ttlSeconds;
    }

    public void setTtlSeconds(Integer ttlSeconds) {
        this.ttlSeconds = ttlSeconds;
    }

    public Integer getPriority() {
        return this.priority;
    }

    public void setPriority(Integer priority) {
        this.priority = priority;
    }

    public String getRecordLine() {
        return this.recordLine;
    }

    public void setRecordLine(String recordLine) {
        this.recordLine = recordLine;
    }

    public String getRecordStatus() {
        return this.recordStatus;
    }

    public void setRecordStatus(String recordStatus) {
        this.recordStatus = recordStatus;
    }

    public String getDomainId() {
        return this.domainId;
    }

    public void setDomainId(String domainId) {
        this.domainId = domainId;
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

    public String getProviderRecordRef() {
        return this.providerRecordRef;
    }

    public void setProviderRecordRef(String providerRecordRef) {
        this.providerRecordRef = providerRecordRef;
    }

    public String getSyncedAt() {
        return this.syncedAt;
    }

    public void setSyncedAt(String syncedAt) {
        this.syncedAt = syncedAt;
    }
}
