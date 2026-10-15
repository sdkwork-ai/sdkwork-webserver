package com.sdkwork.webserver.backend.sdk.model;


public class TunnelMachineRouteDeclarationResponse {
    private String routeId;
    private Boolean active;
    private String publicUrl;
    private String detail;

    public String getRouteId() {
        return this.routeId;
    }

    public void setRouteId(String routeId) {
        this.routeId = routeId;
    }

    public Boolean getActive() {
        return this.active;
    }

    public void setActive(Boolean active) {
        this.active = active;
    }

    public String getPublicUrl() {
        return this.publicUrl;
    }

    public void setPublicUrl(String publicUrl) {
        this.publicUrl = publicUrl;
    }

    public String getDetail() {
        return this.detail;
    }

    public void setDetail(String detail) {
        this.detail = detail;
    }
}
