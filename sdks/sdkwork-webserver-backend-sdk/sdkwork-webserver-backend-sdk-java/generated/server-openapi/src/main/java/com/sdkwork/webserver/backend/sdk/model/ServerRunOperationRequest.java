package com.sdkwork.webserver.backend.sdk.model;


public class ServerRunOperationRequest {
    private String path;
    private String operationId;

    public String getPath() {
        return this.path;
    }

    public void setPath(String path) {
        this.path = path;
    }

    public String getOperationId() {
        return this.operationId;
    }

    public void setOperationId(String operationId) {
        this.operationId = operationId;
    }
}
