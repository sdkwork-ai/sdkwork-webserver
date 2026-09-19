package com.sdkwork.webserver.backend.sdk.model;


public class ClusterOverviewResponse {
    private String totalHosts;
    private String onlineHosts;
    private String totalInstances;
    private String onlineInstances;
    private String unhealthyInstances;
    private String pendingPeerMessages;
    private String generatedAt;

    public String getTotalHosts() {
        return this.totalHosts;
    }

    public void setTotalHosts(String totalHosts) {
        this.totalHosts = totalHosts;
    }

    public String getOnlineHosts() {
        return this.onlineHosts;
    }

    public void setOnlineHosts(String onlineHosts) {
        this.onlineHosts = onlineHosts;
    }

    public String getTotalInstances() {
        return this.totalInstances;
    }

    public void setTotalInstances(String totalInstances) {
        this.totalInstances = totalInstances;
    }

    public String getOnlineInstances() {
        return this.onlineInstances;
    }

    public void setOnlineInstances(String onlineInstances) {
        this.onlineInstances = onlineInstances;
    }

    public String getUnhealthyInstances() {
        return this.unhealthyInstances;
    }

    public void setUnhealthyInstances(String unhealthyInstances) {
        this.unhealthyInstances = unhealthyInstances;
    }

    public String getPendingPeerMessages() {
        return this.pendingPeerMessages;
    }

    public void setPendingPeerMessages(String pendingPeerMessages) {
        this.pendingPeerMessages = pendingPeerMessages;
    }

    public String getGeneratedAt() {
        return this.generatedAt;
    }

    public void setGeneratedAt(String generatedAt) {
        this.generatedAt = generatedAt;
    }
}
