package com.sdkwork.webserver.app.sdk.model;

import java.util.List;

public class SourceVersionPage {
    private List<SourceVersionResponse> items;
    private String total;

    public List<SourceVersionResponse> getItems() {
        return this.items;
    }

    public void setItems(List<SourceVersionResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
