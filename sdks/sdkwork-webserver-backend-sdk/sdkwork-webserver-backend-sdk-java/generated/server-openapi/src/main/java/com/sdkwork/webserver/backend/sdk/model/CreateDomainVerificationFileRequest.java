package com.sdkwork.webserver.backend.sdk.model;


public class CreateDomainVerificationFileRequest {
    private String platform;
    private String fileName;
    private String content;

    public String getPlatform() {
        return this.platform;
    }

    public void setPlatform(String platform) {
        this.platform = platform;
    }

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
