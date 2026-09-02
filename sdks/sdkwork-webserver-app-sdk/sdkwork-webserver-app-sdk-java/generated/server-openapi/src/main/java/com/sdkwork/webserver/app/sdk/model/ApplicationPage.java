package com.sdkwork.webserver.app.sdk.model;

import java.util.List;

public class ApplicationPage {
    private List<ApplicationResponse> items;
    private String total;
    private Integer page;
    private Integer pageSize;

    public List<ApplicationResponse> getItems() {
        return this.items;
    }

    public void setItems(List<ApplicationResponse> items) {
        this.items = items;
    }

    public String getTotal() {
        return this.total;
    }

    public void setTotal(String total) {
        this.total = total;
    }

    public Integer getPage() {
        return this.page;
    }

    public void setPage(Integer page) {
        this.page = page;
    }

    public Integer getPageSize() {
        return this.pageSize;
    }

    public void setPageSize(Integer pageSize) {
        this.pageSize = pageSize;
    }
}
