package com.sdkwork.webserver.backend.sdk.model;


public class WebserverConfigWriteRequest {
    private String content;
    private String expectedSha256;

    public String getContent() {
        return this.content;
    }

    public void setContent(String content) {
        this.content = content;
    }

    public String getExpectedSha256() {
        return this.expectedSha256;
    }

    public void setExpectedSha256(String expectedSha256) {
        this.expectedSha256 = expectedSha256;
    }
}
