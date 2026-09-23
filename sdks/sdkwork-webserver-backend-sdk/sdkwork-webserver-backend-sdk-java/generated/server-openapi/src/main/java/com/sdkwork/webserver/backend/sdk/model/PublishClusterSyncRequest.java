package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class PublishClusterSyncRequest {
    private String kind;
    private Map<String, Object> payload;

    public String getKind() {
        return this.kind;
    }

    public void setKind(String kind) {
        this.kind = kind;
    }

    public Map<String, Object> getPayload() {
        return this.payload;
    }

    public void setPayload(Map<String, Object> payload) {
        this.payload = payload;
    }
}
