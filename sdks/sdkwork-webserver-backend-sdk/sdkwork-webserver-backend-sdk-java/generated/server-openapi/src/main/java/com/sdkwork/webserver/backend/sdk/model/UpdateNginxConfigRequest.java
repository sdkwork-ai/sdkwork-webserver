package com.sdkwork.webserver.backend.sdk.model;


public class UpdateNginxConfigRequest {
    private String configContent;
    private String configName;

    public String getConfigContent() {
        return this.configContent;
    }

    public void setConfigContent(String configContent) {
        this.configContent = configContent;
    }

    public String getConfigName() {
        return this.configName;
    }

    public void setConfigName(String configName) {
        this.configName = configName;
    }
}
