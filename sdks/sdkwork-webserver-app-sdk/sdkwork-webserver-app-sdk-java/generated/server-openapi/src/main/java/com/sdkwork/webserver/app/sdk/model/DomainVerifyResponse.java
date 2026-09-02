package com.sdkwork.webserver.app.sdk.model;


public class DomainVerifyResponse {
    private Boolean verified;
    private String status;
    private String method;
    private String recordName;
    private String recordValue;
    private Integer attemptCount;
    private String expiresAt;
    private String nextAttemptAt;
    private String checkedAt;
    private String failureCode;

    public Boolean getVerified() {
        return this.verified;
    }

    public void setVerified(Boolean verified) {
        this.verified = verified;
    }

    public String getStatus() {
        return this.status;
    }

    public void setStatus(String status) {
        this.status = status;
    }

    public String getMethod() {
        return this.method;
    }

    public void setMethod(String method) {
        this.method = method;
    }

    public String getRecordName() {
        return this.recordName;
    }

    public void setRecordName(String recordName) {
        this.recordName = recordName;
    }

    public String getRecordValue() {
        return this.recordValue;
    }

    public void setRecordValue(String recordValue) {
        this.recordValue = recordValue;
    }

    public Integer getAttemptCount() {
        return this.attemptCount;
    }

    public void setAttemptCount(Integer attemptCount) {
        this.attemptCount = attemptCount;
    }

    public String getExpiresAt() {
        return this.expiresAt;
    }

    public void setExpiresAt(String expiresAt) {
        this.expiresAt = expiresAt;
    }

    public String getNextAttemptAt() {
        return this.nextAttemptAt;
    }

    public void setNextAttemptAt(String nextAttemptAt) {
        this.nextAttemptAt = nextAttemptAt;
    }

    public String getCheckedAt() {
        return this.checkedAt;
    }

    public void setCheckedAt(String checkedAt) {
        this.checkedAt = checkedAt;
    }

    public String getFailureCode() {
        return this.failureCode;
    }

    public void setFailureCode(String failureCode) {
        this.failureCode = failureCode;
    }
}
