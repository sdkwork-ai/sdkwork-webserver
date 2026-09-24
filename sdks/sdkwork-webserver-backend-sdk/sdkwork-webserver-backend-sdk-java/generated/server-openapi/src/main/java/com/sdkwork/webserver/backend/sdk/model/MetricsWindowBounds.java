package com.sdkwork.webserver.backend.sdk.model;


public class MetricsWindowBounds {
    private String window;
    private String dateFrom;
    private String dateTo;

    public String getWindow() {
        return this.window;
    }

    public void setWindow(String window) {
        this.window = window;
    }

    public String getDateFrom() {
        return this.dateFrom;
    }

    public void setDateFrom(String dateFrom) {
        this.dateFrom = dateFrom;
    }

    public String getDateTo() {
        return this.dateTo;
    }

    public void setDateTo(String dateTo) {
        this.dateTo = dateTo;
    }
}
