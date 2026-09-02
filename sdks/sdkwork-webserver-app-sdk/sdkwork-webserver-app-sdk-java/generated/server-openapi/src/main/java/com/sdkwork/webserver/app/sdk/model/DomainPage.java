package com.sdkwork.webserver.app.sdk.model;

import java.util.List;

public class DomainPage {
    private List<DomainResponse> items;
    private String total;

    public List<DomainResponse> getItems() {
        return this.items;
    }

    public void setItems(List<DomainResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
