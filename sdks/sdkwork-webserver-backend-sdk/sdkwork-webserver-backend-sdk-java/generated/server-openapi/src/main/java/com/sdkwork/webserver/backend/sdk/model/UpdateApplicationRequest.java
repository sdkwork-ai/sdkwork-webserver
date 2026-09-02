package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class UpdateApplicationRequest {
    private String name;
    private String description;
    private Map<String, Object> runtimeConfig;
    private ApplicationStoreListing storeListing;

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public String getDescription() {
        return this.description;
    }

    public void setDescription(String description) {
        this.description = description;
    }

    public Map<String, Object> getRuntimeConfig() {
        return this.runtimeConfig;
    }

    public void setRuntimeConfig(Map<String, Object> runtimeConfig) {
        this.runtimeConfig = runtimeConfig;
    }

    public ApplicationStoreListing getStoreListing() {
        return this.storeListing;
    }

    public void setStoreListing(ApplicationStoreListing storeListing) {
        this.storeListing = storeListing;
    }
}
