package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class CreatePlatformTargetRequest {
    private String targetKey;
    private String platform;
    private String techStack;
    private List<String> architectures;
    private String bundleId;
    private String packageName;
    private String appId;
    private String bundleName;
    private List<String> allowedChannels;

    public String getTargetKey() {
        return this.targetKey;
    }

    public void setTargetKey(String targetKey) {
        this.targetKey = targetKey;
    }

    public String getPlatform() {
        return this.platform;
    }

    public void setPlatform(String platform) {
        this.platform = platform;
    }

    public String getTechStack() {
        return this.techStack;
    }

    public void setTechStack(String techStack) {
        this.techStack = techStack;
    }

    public List<String> getArchitectures() {
        return this.architectures;
    }

    public void setArchitectures(List<String> architectures) {
        this.architectures = architectures;
    }

    public String getBundleId() {
        return this.bundleId;
    }

    public void setBundleId(String bundleId) {
        this.bundleId = bundleId;
    }

    public String getPackageName() {
        return this.packageName;
    }

    public void setPackageName(String packageName) {
        this.packageName = packageName;
    }

    public String getAppId() {
        return this.appId;
    }

    public void setAppId(String appId) {
        this.appId = appId;
    }

    public String getBundleName() {
        return this.bundleName;
    }

    public void setBundleName(String bundleName) {
        this.bundleName = bundleName;
    }

    public List<String> getAllowedChannels() {
        return this.allowedChannels;
    }

    public void setAllowedChannels(List<String> allowedChannels) {
        this.allowedChannels = allowedChannels;
    }
}
