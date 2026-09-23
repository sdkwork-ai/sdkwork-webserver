package com.sdkwork.webserver.backend.sdk.model;


public class TrafficUsageDailyPoint {
    private String usageDate;
    private String dimension;
    private String quantity;

    public String getUsageDate() {
        return this.usageDate;
    }

    public void setUsageDate(String usageDate) {
        this.usageDate = usageDate;
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
}
