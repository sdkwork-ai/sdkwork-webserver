package com.sdkwork.web.app.sdk.model;

import java.util.List;

public class PlatformTargetResponse {
    private String id;
    private String appId;
    private String targetKey;
    private String platform;
    private String techStack;
    private List<String> architectures;
    private String bundleId;
    private String packageName;
    private String appIdValue;
    private String bundleName;
    private String targetStatus;
    private String createdAt;
    private String updatedAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getAppId() {
        return this.appId;
    }

    public void setAppId(String appId) {
        this.appId = appId;
    }

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

    public String getAppIdValue() {
        return this.appIdValue;
    }

    public void setAppIdValue(String appIdValue) {
        this.appIdValue = appIdValue;
    }

    public String getBundleName() {
        return this.bundleName;
    }

    public void setBundleName(String bundleName) {
        this.bundleName = bundleName;
    }

    public String getTargetStatus() {
        return this.targetStatus;
    }

    public void setTargetStatus(String targetStatus) {
        this.targetStatus = targetStatus;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }

    public String getUpdatedAt() {
        return this.updatedAt;
    }

    public void setUpdatedAt(String updatedAt) {
        this.updatedAt = updatedAt;
    }
}
