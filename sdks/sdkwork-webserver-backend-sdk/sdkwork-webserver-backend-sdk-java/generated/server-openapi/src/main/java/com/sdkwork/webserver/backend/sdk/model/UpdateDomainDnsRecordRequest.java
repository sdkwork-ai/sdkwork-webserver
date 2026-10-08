package com.sdkwork.webserver.backend.sdk.model;


public class UpdateDomainDnsRecordRequest {
    private String recordType;
    private String host;
    private String recordValue;
    private Integer ttlSeconds;
    private Integer priority;
    private String recordLine;

    public String getRecordType() {
        return this.recordType;
    }

    public void setRecordType(String recordType) {
        this.recordType = recordType;
    }

    public String getHost() {
        return this.host;
    }

    public void setHost(String host) {
        this.host = host;
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
}
