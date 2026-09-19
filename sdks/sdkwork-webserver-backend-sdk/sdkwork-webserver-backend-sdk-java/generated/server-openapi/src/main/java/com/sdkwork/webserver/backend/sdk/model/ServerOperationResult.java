package com.sdkwork.webserver.backend.sdk.model;


public class ServerOperationResult {
    private String operationId;
    private Integer exitCode;
    private Boolean timedOut;
    private String stdout;
    private String stderr;
    private Boolean stdoutTruncated;
    private Boolean stderrTruncated;
    private Integer pid;
    private String pidFile;
    private String logFile;
    private Boolean stopped;
    private String message;

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

    public Boolean getTimedOut() {
        return this.timedOut;
    }

    public void setTimedOut(Boolean timedOut) {
        this.timedOut = timedOut;
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

    public Boolean getStdoutTruncated() {
        return this.stdoutTruncated;
    }

    public void setStdoutTruncated(Boolean stdoutTruncated) {
        this.stdoutTruncated = stdoutTruncated;
    }

    public Boolean getStderrTruncated() {
        return this.stderrTruncated;
    }

    public void setStderrTruncated(Boolean stderrTruncated) {
        this.stderrTruncated = stderrTruncated;
    }

    public Integer getPid() {
        return this.pid;
    }

    public void setPid(Integer pid) {
        this.pid = pid;
    }

    public String getPidFile() {
        return this.pidFile;
    }

    public void setPidFile(String pidFile) {
        this.pidFile = pidFile;
    }

    public String getLogFile() {
        return this.logFile;
    }

    public void setLogFile(String logFile) {
        this.logFile = logFile;
    }

    public Boolean getStopped() {
        return this.stopped;
    }

    public void setStopped(Boolean stopped) {
        this.stopped = stopped;
    }

    public String getMessage() {
        return this.message;
    }

    public void setMessage(String message) {
        this.message = message;
    }
}
