package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class ServerDirectoryListing {
    private String nodeId;
    private String path;
    private String parentPath;
    private List<ServerEntry> entries;

    public String getNodeId() {
        return this.nodeId;
    }

    public void setNodeId(String nodeId) {
        this.nodeId = nodeId;
    }

    public String getPath() {
        return this.path;
    }

    public void setPath(String path) {
        this.path = path;
    }

    public String getParentPath() {
        return this.parentPath;
    }

    public void setParentPath(String parentPath) {
        this.parentPath = parentPath;
    }

    public List<ServerEntry> getEntries() {
        return this.entries;
    }

    public void setEntries(List<ServerEntry> entries) {
        this.entries = entries;
    }
}
