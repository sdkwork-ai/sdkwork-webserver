package com.sdkwork.webserver.backend.sdk.model;


public class UpdateClusterInstanceRequest {
    private String name;
    private Integer status;
    private String publicEndpoint;

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getPublicEndpoint() {
        return this.publicEndpoint;
    }

    public void setPublicEndpoint(String publicEndpoint) {
        this.publicEndpoint = publicEndpoint;
    }
}
