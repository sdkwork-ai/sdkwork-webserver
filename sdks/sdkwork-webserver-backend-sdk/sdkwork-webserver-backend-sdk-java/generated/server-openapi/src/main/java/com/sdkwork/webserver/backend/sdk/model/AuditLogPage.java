package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class AuditLogPage {
    private List<AuditLogResponse> items;
    private String total;

    public List<AuditLogResponse> getItems() {
        return this.items;
    }

    public void setItems(List<AuditLogResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }
}
