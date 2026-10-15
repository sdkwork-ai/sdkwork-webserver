package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class TunnelMachineResponse {
    private String deviceId;
    private String deviceName;
    private String platform;
    private Boolean online;
    private Boolean enabled;
    private String sessionId;
    private String firstSeenAt;
    private String lastSeenAt;
    private String connectedAt;
    private String disconnectedAt;
    private String remoteAddr;
    private String bytesIn;
    private String bytesOut;
    private String sessionsTotal;
    private String routeCount;
    private String healthyRouteCount;
    private List<String> unhealthyRouteIds;
    private List<TunnelMachineRouteResponse> routes;

    public String getDeviceId() {
        return this.deviceId;
    }

    public void setDeviceId(String deviceId) {
        this.deviceId = deviceId;
    }

    public String getDeviceName() {
        return this.deviceName;
    }

    public void setDeviceName(String deviceName) {
        this.deviceName = deviceName;
    }

    public String getPlatform() {
        return this.platform;
    }

    public void setPlatform(String platform) {
        this.platform = platform;
    }

    public Boolean getOnline() {
        return this.online;
    }

    public void setOnline(Boolean online) {
        this.online = online;
    }

    public Boolean getEnabled() {
        return this.enabled;
    }

    public void setEnabled(Boolean enabled) {
        this.enabled = enabled;
    }

    public String getSessionId() {
        return this.sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public String getFirstSeenAt() {
        return this.firstSeenAt;
    }

    public void setFirstSeenAt(String firstSeenAt) {
        this.firstSeenAt = firstSeenAt;
    }

    public String getLastSeenAt() {
        return this.lastSeenAt;
    }

    public void setLastSeenAt(String lastSeenAt) {
        this.lastSeenAt = lastSeenAt;
    }

    public String getConnectedAt() {
        return this.connectedAt;
    }

    public void setConnectedAt(String connectedAt) {
        this.connectedAt = connectedAt;
    }

    public String getDisconnectedAt() {
        return this.disconnectedAt;
    }

    public void setDisconnectedAt(String disconnectedAt) {
        this.disconnectedAt = disconnectedAt;
    }

    public String getRemoteAddr() {
        return this.remoteAddr;
    }

    public void setRemoteAddr(String remoteAddr) {
        this.remoteAddr = remoteAddr;
    }

    public String getBytesIn() {
        return this.bytesIn;
    }

    public void setBytesIn(String bytesIn) {
        this.bytesIn = bytesIn;
    }

    public String getBytesOut() {
        return this.bytesOut;
    }

    public void setBytesOut(String bytesOut) {
        this.bytesOut = bytesOut;
    }

    public String getSessionsTotal() {
        return this.sessionsTotal;
    }

    public void setSessionsTotal(String sessionsTotal) {
        this.sessionsTotal = sessionsTotal;
    }

    public String getRouteCount() {
        return this.routeCount;
    }

    public void setRouteCount(String routeCount) {
        this.routeCount = routeCount;
    }

    public String getHealthyRouteCount() {
        return this.healthyRouteCount;
    }

    public void setHealthyRouteCount(String healthyRouteCount) {
        this.healthyRouteCount = healthyRouteCount;
    }

    public List<String> getUnhealthyRouteIds() {
        return this.unhealthyRouteIds;
    }

    public void setUnhealthyRouteIds(List<String> unhealthyRouteIds) {
        this.unhealthyRouteIds = unhealthyRouteIds;
    }

    public List<TunnelMachineRouteResponse> getRoutes() {
        return this.routes;
    }

    public void setRoutes(List<TunnelMachineRouteResponse> routes) {
        this.routes = routes;
    }
}
