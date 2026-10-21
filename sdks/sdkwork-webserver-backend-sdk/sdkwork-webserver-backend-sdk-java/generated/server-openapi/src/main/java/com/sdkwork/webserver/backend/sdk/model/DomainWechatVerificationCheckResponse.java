package com.sdkwork.webserver.backend.sdk.model;


public class DomainWechatVerificationCheckResponse {
    private Boolean reachable;
    private Boolean matched;
    private Integer statusCode;
    private String scheme;
    private String detail;
    private String checkedAt;

    public Boolean getReachable() {
        return this.reachable;
    }

    public void setReachable(Boolean reachable) {
        this.reachable = reachable;
    }

    public Boolean getMatched() {
        return this.matched;
    }

    public void setMatched(Boolean matched) {
        this.matched = matched;
    }

    public Integer getStatusCode() {
        return this.statusCode;
    }

    public void setStatusCode(Integer statusCode) {
        this.statusCode = statusCode;
    }

    public String getScheme() {
        return this.scheme;
    }

    public void setScheme(String scheme) {
        this.scheme = scheme;
    }

    public String getDetail() {
        return this.detail;
    }

    public void setDetail(String detail) {
        this.detail = detail;
    }

    public String getCheckedAt() {
        return this.checkedAt;
    }

    public void setCheckedAt(String checkedAt) {
        this.checkedAt = checkedAt;
    }
}
