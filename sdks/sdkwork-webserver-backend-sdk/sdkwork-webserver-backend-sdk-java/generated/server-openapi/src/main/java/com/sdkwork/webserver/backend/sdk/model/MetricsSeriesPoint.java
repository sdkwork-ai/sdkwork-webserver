package com.sdkwork.webserver.backend.sdk.model;


public class MetricsSeriesPoint {
    private String date;
    private String quantity;

    public String getDate() {
        return this.date;
    }

    public void setDate(String date) {
        this.date = date;
    }

    public String getQuantity() {
        return this.quantity;
    }

    public void setQuantity(String quantity) {
        this.quantity = quantity;
    }
}
