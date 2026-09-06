package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class WebserverConfigCatalog {
    private String configRoot;
    private List<WebserverConfigEntry> items;

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
}
