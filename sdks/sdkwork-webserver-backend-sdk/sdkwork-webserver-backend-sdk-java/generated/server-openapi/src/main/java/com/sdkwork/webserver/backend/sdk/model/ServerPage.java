package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class ServerPage {
    private List<ServerResponse> items;
    private String total;

    public List<ServerResponse> getItems() {
        return this.items;
    }

    public void setItems(List<ServerResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
