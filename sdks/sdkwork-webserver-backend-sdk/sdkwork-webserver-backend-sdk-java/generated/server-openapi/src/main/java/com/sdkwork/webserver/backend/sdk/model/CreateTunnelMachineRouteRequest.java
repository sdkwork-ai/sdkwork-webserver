package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class CreateTunnelMachineRouteRequest {
    private String name;
    private String protocol;
    private String domain;
    private Integer port;
    private Boolean allowPublic;
    private List<String> allowedIps;
    private String auth;
    private List<String> visitorTokens;
    private List<String> locations;
    private String bandwidthLimitBytesPerSecond;
    private String proxyProtocol;
    private String loadBalancerGroup;

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

    public Boolean getAllowPublic() {
        return this.allowPublic;
    }

    public void setAllowPublic(Boolean allowPublic) {
        this.allowPublic = allowPublic;
    }

    public List<String> getAllowedIps() {
        return this.allowedIps;
    }

    public void setAllowedIps(List<String> allowedIps) {
        this.allowedIps = allowedIps;
    }

    public String getAuth() {
        return this.auth;
    }

    public void setAuth(String auth) {
        this.auth = auth;
    }

    public List<String> getVisitorTokens() {
        return this.visitorTokens;
    }

    public void setVisitorTokens(List<String> visitorTokens) {
        this.visitorTokens = visitorTokens;
    }

    public List<String> getLocations() {
        return this.locations;
    }

    public void setLocations(List<String> locations) {
        this.locations = locations;
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

    public String getLoadBalancerGroup() {
        return this.loadBalancerGroup;
    }

    public void setLoadBalancerGroup(String loadBalancerGroup) {
        this.loadBalancerGroup = loadBalancerGroup;
    }
}
