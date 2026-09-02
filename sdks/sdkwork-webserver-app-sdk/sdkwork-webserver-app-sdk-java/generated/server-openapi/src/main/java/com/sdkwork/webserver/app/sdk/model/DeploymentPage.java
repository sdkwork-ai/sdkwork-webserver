package com.sdkwork.webserver.app.sdk.model;

import java.util.List;

public class DeploymentPage {
    private List<DeploymentResponse> items;
    private String total;

    public List<DeploymentResponse> getItems() {
        return this.items;
    }

    public void setItems(List<DeploymentResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
