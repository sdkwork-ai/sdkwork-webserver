package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class NginxDeployResponse {
    private Boolean success;
    private String configId;
    private String deployedAt;
    private Map<String, Object> reloadResult;

    public Boolean getSuccess() {
        return this.success;
    }

    public void setSuccess(Boolean success) {
        this.success = success;
    }

    public String getConfigId() {
        return this.configId;
    }

    public void setConfigId(String configId) {
        this.configId = configId;
    }

    public String getDeployedAt() {
        return this.deployedAt;
    }

    public void setDeployedAt(String deployedAt) {
        this.deployedAt = deployedAt;
    }

    public Map<String, Object> getReloadResult() {
        return this.reloadResult;
    }

    public void setReloadResult(Map<String, Object> reloadResult) {
        this.reloadResult = reloadResult;
    }
}
