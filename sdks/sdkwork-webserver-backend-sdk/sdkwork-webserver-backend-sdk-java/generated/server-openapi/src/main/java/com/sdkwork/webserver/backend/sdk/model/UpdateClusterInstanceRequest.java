package com.sdkwork.webserver.backend.sdk.model;

import java.util.Map;

public class UpdateClusterInstanceRequest {
    private String name;
    private Integer status;
    private String publicEndpoint;
    private Boolean routingEnabled;
    private Boolean draining;
    private String probeUrl;
    private Map<String, String> labels;
    private Integer routingWeight;
    private String maintenanceNote;

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getPublicEndpoint() {
        return this.publicEndpoint;
    }

    public void setPublicEndpoint(String publicEndpoint) {
        this.publicEndpoint = publicEndpoint;
    }

    public Boolean getRoutingEnabled() {
        return this.routingEnabled;
    }

    public void setRoutingEnabled(Boolean routingEnabled) {
        this.routingEnabled = routingEnabled;
    }

    public Boolean getDraining() {
        return this.draining;
    }

    public void setDraining(Boolean draining) {
        this.draining = draining;
    }

    public String getProbeUrl() {
        return this.probeUrl;
    }

    public void setProbeUrl(String probeUrl) {
        this.probeUrl = probeUrl;
    }

    public Map<String, String> getLabels() {
        return this.labels;
    }

    public void setLabels(Map<String, String> labels) {
        this.labels = labels;
    }

    public Integer getRoutingWeight() {
        return this.routingWeight;
    }

    public void setRoutingWeight(Integer routingWeight) {
        this.routingWeight = routingWeight;
    }

    public String getMaintenanceNote() {
        return this.maintenanceNote;
    }

    public void setMaintenanceNote(String maintenanceNote) {
        this.maintenanceNote = maintenanceNote;
    }
}
