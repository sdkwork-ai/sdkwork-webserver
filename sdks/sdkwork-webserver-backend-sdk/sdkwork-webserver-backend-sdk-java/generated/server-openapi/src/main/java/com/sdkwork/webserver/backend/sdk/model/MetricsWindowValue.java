package com.sdkwork.webserver.backend.sdk.model;


public class MetricsWindowValue {
    private String window;
    private String quantity;
    private String unit;

    public String getWindow() {
        return this.window;
    }

    public void setWindow(String window) {
        this.window = window;
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
