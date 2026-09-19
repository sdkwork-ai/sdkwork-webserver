package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class EnqueueClusterPeerMessagesRequest {
    private String clusterId;
    private String toInstanceId;
    private String fromInstanceId;
    private String messageType;
    private Map<String, Object> payload;
    private Integer expiresInSeconds;

    public String getClusterId() {
        return this.clusterId;
    }

    public void setClusterId(String clusterId) {
        this.clusterId = clusterId;
    }

    public String getToInstanceId() {
        return this.toInstanceId;
    }

    public void setToInstanceId(String toInstanceId) {
        this.toInstanceId = toInstanceId;
    }

    public String getFromInstanceId() {
        return this.fromInstanceId;
    }

    public void setFromInstanceId(String fromInstanceId) {
        this.fromInstanceId = fromInstanceId;
    }

    public String getMessageType() {
        return this.messageType;
    }

    public void setMessageType(String messageType) {
        this.messageType = messageType;
    }

    public Map<String, Object> getPayload() {
        return this.payload;
    }

    public void setPayload(Map<String, Object> payload) {
        this.payload = payload;
    }

    public Integer getExpiresInSeconds() {
        return this.expiresInSeconds;
    }

    public void setExpiresInSeconds(Integer expiresInSeconds) {
        this.expiresInSeconds = expiresInSeconds;
    }
}
