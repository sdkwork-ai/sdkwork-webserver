package com.sdkwork.webserver.backend.sdk.model;


public class ServerEntry {
    private String name;
    private String kind;
    private String path;
    private String size;
    private String projectType;
    private Boolean isProjectRoot;

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public String getKind() {
        return this.kind;
    }

    public void setKind(String kind) {
        this.kind = kind;
    }

    public String getPath() {
        return this.path;
    }

    public void setPath(String path) {
        this.path = path;
    }

    public String getSize() {
        return this.size;
    }

    public void setSize(String size) {
        this.size = size;
    }

    public String getProjectType() {
        return this.projectType;
    }

    public void setProjectType(String projectType) {
        this.projectType = projectType;
    }

    public Boolean getIsProjectRoot() {
        return this.isProjectRoot;
    }

    public void setIsProjectRoot(Boolean isProjectRoot) {
        this.isProjectRoot = isProjectRoot;
    }
}
