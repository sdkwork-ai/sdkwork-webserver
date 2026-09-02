package com.sdkwork.webserver.backend.sdk.model;


public class ServerOperationResult {
    private String operationId;
    private Integer exitCode;
    private String stdout;
    private String stderr;

    public String getOperationId() {
        return this.operationId;
    }

    public void setOperationId(String operationId) {
        this.operationId = operationId;
    }

    public Integer getExitCode() {
        return this.exitCode;
    }

    public void setExitCode(Integer exitCode) {
        this.exitCode = exitCode;
    }

    public String getStdout() {
        return this.stdout;
    }

    public void setStdout(String stdout) {
        this.stdout = stdout;
    }

    public String getStderr() {
        return this.stderr;
    }

    public void setStderr(String stderr) {
        this.stderr = stderr;
    }
}
