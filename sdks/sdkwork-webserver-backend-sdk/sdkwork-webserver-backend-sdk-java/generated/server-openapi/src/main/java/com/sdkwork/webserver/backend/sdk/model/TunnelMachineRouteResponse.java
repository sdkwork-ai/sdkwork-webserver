package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class TunnelMachineRouteResponse {
    private String routeId;
    private String name;
    private String protocol;
    private String domain;
    private Integer port;
    private String secretName;
    private List<String> locations;
    private String loadBalancerGroup;
    private String bandwidthLimitBytesPerSecond;
    private String proxyProtocol;
    private Boolean allowPublic;
    private String auth;
    private List<String> allowedIps;
    private Boolean healthy;

    public String getRouteId() {
        return this.routeId;
    }

    public void setRouteId(String routeId) {
        this.routeId = routeId;
    }

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public String getProtocol() {
        return this.protocol;
    }

    public void setProtocol(String protocol) {
        this.protocol = protocol;
    }

    public String getDomain() {
        return this.domain;
    }

    public void setDomain(String domain) {
        this.domain = domain;
    }

    public Integer getPort() {
        return this.port;
    }

    public void setPort(Integer port) {
        this.port = port;
    }

    public String getSecretName() {
        return this.secretName;
    }

    public void setSecretName(String secretName) {
        this.secretName = secretName;
    }

    public List<String> getLocations() {
        return this.locations;
    }

    public void setLocations(List<String> locations) {
        this.locations = locations;
    }

    public String getLoadBalancerGroup() {
        return this.loadBalancerGroup;
    }

    public void setLoadBalancerGroup(String loadBalancerGroup) {
        this.loadBalancerGroup = loadBalancerGroup;
    }

    public String getBandwidthLimitBytesPerSecond() {
        return this.bandwidthLimitBytesPerSecond;
    }

    public void setBandwidthLimitBytesPerSecond(String bandwidthLimitBytesPerSecond) {
        this.bandwidthLimitBytesPerSecond = bandwidthLimitBytesPerSecond;
    }

    public String getProxyProtocol() {
        return this.proxyProtocol;
    }

    public void setProxyProtocol(String proxyProtocol) {
        this.proxyProtocol = proxyProtocol;
    }

    public Boolean getAllowPublic() {
        return this.allowPublic;
    }

    public void setAllowPublic(Boolean allowPublic) {
        this.allowPublic = allowPublic;
    }

    public String getAuth() {
        return this.auth;
    }

    public void setAuth(String auth) {
        this.auth = auth;
    }

    public List<String> getAllowedIps() {
        return this.allowedIps;
    }

    public void setAllowedIps(List<String> allowedIps) {
        this.allowedIps = allowedIps;
    }

    public Boolean getHealthy() {
        return this.healthy;
    }

    public void setHealthy(Boolean healthy) {
        this.healthy = healthy;
    }
}
