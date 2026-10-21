package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class WebserverConfigCatalog {
    private String configRoot;
    private List<WebserverConfigEntry> items;
    private Boolean truncated;
    private PageInfo pageInfo;

    public String getConfigRoot() {
        return this.configRoot;
    }

    public void setConfigRoot(String configRoot) {
        this.configRoot = configRoot;
    }

    public List<WebserverConfigEntry> getItems() {
        return this.items;
    }

    public void setItems(List<WebserverConfigEntry> items) {
        this.items = items;
    }

    public Boolean getTruncated() {
        return this.truncated;
    }

    public void setTruncated(Boolean truncated) {
        this.truncated = truncated;
    }

    public PageInfo getPageInfo() {
        return this.pageInfo;
    }

    public void setPageInfo(PageInfo pageInfo) {
        this.pageInfo = pageInfo;
    }
}
