package com.sdkwork.webserver.backend.sdk.model;


public class ProbeClusterInstanceRequest {
    private String path;
    private Integer timeoutMs;

    public String getPath() {
        return this.path;
    }

    public void setPath(String path) {
        this.path = path;
    }

    public Integer getTimeoutMs() {
        return this.timeoutMs;
    }

    public void setTimeoutMs(Integer timeoutMs) {
        this.timeoutMs = timeoutMs;
    }
}
