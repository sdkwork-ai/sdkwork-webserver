package com.sdkwork.webserver.backend.sdk.model;


public class UpdateClusterHostRequest {
    private String name;
    private String clusterId;

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public String getClusterId() {
        return this.clusterId;
    }

    public void setClusterId(String clusterId) {
        this.clusterId = clusterId;
    }
}
