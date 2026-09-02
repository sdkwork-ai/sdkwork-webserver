package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class NginxConfigPage {
    private List<NginxConfigResponse> items;
    private String total;

    public List<NginxConfigResponse> getItems() {
        return this.items;
    }

    public void setItems(List<NginxConfigResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
