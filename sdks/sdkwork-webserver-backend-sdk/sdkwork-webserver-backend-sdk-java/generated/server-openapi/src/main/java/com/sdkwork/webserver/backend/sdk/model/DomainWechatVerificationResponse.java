package com.sdkwork.webserver.backend.sdk.model;


public class DomainWechatVerificationResponse {
    private String rootDomainId;
    private String fileName;
    private String content;
    private String updatedAt;

    public String getRootDomainId() {
        return this.rootDomainId;
    }

    public void setRootDomainId(String rootDomainId) {
        this.rootDomainId = rootDomainId;
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

    public String getUpdatedAt() {
        return this.updatedAt;
    }

    public void setUpdatedAt(String updatedAt) {
        this.updatedAt = updatedAt;
    }
}
