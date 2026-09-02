package com.sdkwork.webserver.app.sdk.model;

import java.util.List;
import java.util.Map;

public class SdkWorkPageData {
    private List<Map<String, Object>> items;
    private PageInfo pageInfo;

    public List<Map<String, Object>> getItems() {
        return this.items;
    }

    public void setItems(List<Map<String, Object>> items) {
        this.items = items;
    }

    public PageInfo getPageInfo() {
        return this.pageInfo;
    }

    public void setPageInfo(PageInfo pageInfo) {
        this.pageInfo = pageInfo;
    }
}
