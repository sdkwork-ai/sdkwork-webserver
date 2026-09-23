package com.sdkwork.webserver.backend.sdk.model;


public class TrafficUsageAppTotal {
    private String appUuid;
    private String appSlug;
    private String dimension;
    private String quantity;
    private String unit;

    public String getAppUuid() {
        return this.appUuid;
    }

    public void setAppUuid(String appUuid) {
        this.appUuid = appUuid;
    }

    public String getAppSlug() {
        return this.appSlug;
    }

    public void setAppSlug(String appSlug) {
        this.appSlug = appSlug;
    }

    public String getDimension() {
        return this.dimension;
    }

    public void setDimension(String dimension) {
        this.dimension = dimension;
    }

    public String getQuantity() {
        return this.quantity;
    }

    public void setQuantity(String quantity) {
        this.quantity = quantity;
    }

    public String getUnit() {
        return this.unit;
    }

    public void setUnit(String unit) {
        this.unit = unit;
    }
}
