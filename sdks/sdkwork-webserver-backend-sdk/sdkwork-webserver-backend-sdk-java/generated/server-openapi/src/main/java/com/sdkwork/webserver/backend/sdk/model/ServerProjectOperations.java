package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class ServerProjectOperations {
    private String nodeId;
    private String path;
    private String projectType;
    private List<ServerProjectOperation> operations;

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

    public String getProjectType() {
        return this.projectType;
    }

    public void setProjectType(String projectType) {
        this.projectType = projectType;
    }

    public List<ServerProjectOperation> getOperations() {
        return this.operations;
    }

    public void setOperations(List<ServerProjectOperation> operations) {
        this.operations = operations;
    }
}
