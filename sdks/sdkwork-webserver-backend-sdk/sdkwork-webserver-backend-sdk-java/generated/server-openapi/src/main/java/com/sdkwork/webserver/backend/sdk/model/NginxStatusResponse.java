package com.sdkwork.webserver.backend.sdk.model;


public class NginxStatusResponse {
    private Boolean running;
    private String version;
    private Integer pid;
    private Integer activeConnections;
    private String configPath;
    private String uptime;

    public Boolean getRunning() {
        return this.running;
    }

    public void setRunning(Boolean running) {
        this.running = running;
    }

    public String getVersion() {
        return this.version;
    }

    public void setVersion(String version) {
        this.version = version;
    }

    public Integer getPid() {
        return this.pid;
    }

    public void setPid(Integer pid) {
        this.pid = pid;
    }

    public Integer getActiveConnections() {
        return this.activeConnections;
    }

    public void setActiveConnections(Integer activeConnections) {
        this.activeConnections = activeConnections;
    }

    public String getConfigPath() {
        return this.configPath;
    }

    public void setConfigPath(String configPath) {
        this.configPath = configPath;
    }

    public String getUptime() {
        return this.uptime;
    }

    public void setUptime(String uptime) {
        this.uptime = uptime;
    }
}
