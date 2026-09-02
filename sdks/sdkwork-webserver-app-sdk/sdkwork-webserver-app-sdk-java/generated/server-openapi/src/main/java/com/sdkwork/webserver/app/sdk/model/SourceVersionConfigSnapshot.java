package com.sdkwork.webserver.app.sdk.model;


public class SourceVersionConfigSnapshot {
    private String appConfigPath;
    private String deploymentConfigPath;
    private Boolean appConfigDetected;
    private Boolean deploymentConfigDetected;

    public String getAppConfigPath() {
        return this.appConfigPath;
    }

    public void setAppConfigPath(String appConfigPath) {
        this.appConfigPath = appConfigPath;
    }

    public String getDeploymentConfigPath() {
        return this.deploymentConfigPath;
    }

    public void setDeploymentConfigPath(String deploymentConfigPath) {
        this.deploymentConfigPath = deploymentConfigPath;
    }

    public Boolean getAppConfigDetected() {
        return this.appConfigDetected;
    }

    public void setAppConfigDetected(Boolean appConfigDetected) {
        this.appConfigDetected = appConfigDetected;
    }

    public Boolean getDeploymentConfigDetected() {
        return this.deploymentConfigDetected;
    }

    public void setDeploymentConfigDetected(Boolean deploymentConfigDetected) {
        this.deploymentConfigDetected = deploymentConfigDetected;
    }
}
