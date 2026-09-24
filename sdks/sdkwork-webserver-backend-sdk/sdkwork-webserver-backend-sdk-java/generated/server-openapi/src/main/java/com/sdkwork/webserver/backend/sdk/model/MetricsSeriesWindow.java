package com.sdkwork.webserver.backend.sdk.model;


public class MetricsSeriesWindow {
    private String dateFrom;
    private String dateTo;

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
