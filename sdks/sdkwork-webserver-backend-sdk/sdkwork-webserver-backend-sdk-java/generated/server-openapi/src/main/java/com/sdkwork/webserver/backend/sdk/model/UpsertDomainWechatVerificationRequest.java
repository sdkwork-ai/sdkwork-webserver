package com.sdkwork.webserver.backend.sdk.model;


public class UpsertDomainWechatVerificationRequest {
    private String fileName;
    private String content;

    public String getFileName() {
        return this.fileName;
    }

    public void setFileName(String fileName) {
        this.fileName = fileName;
    }

    public String getContent() {
        return this.content;
    }

    public void setContent(String content) {
        this.content = content;
    }
}
