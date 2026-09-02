package com.sdkwork.webserver.app.sdk.model;

import java.util.List;

public class HealthCheckPage {
    private List<HealthCheckResponse> items;
    private String total;

    public List<HealthCheckResponse> getItems() {
        return this.items;
    }

    public void setItems(List<HealthCheckResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
