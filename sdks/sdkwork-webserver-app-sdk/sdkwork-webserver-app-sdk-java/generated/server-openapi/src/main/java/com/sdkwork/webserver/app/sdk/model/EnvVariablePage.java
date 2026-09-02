package com.sdkwork.webserver.app.sdk.model;

import java.util.List;

public class EnvVariablePage {
    private List<EnvVariableResponse> items;
    private String total;

    public List<EnvVariableResponse> getItems() {
        return this.items;
    }

    public void setItems(List<EnvVariableResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
