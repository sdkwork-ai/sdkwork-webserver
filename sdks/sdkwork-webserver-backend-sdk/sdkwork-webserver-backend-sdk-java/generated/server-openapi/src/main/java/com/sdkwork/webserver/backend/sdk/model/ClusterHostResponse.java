package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class ClusterHostResponse {
    private String id;
    private String clusterId;
    private String name;
    private String hostname;
    private String machineCode;
    private String osName;
    private String osVersion;
    private String kernelVersion;
    private String arch;
    private String cpuModel;
    private Integer cpuCores;
    private String memoryTotalMb;
    private String remoteIp;
    private List<String> localIps;
    private List<String> macAddresses;
    private String daemonVersion;
    private Integer status;
    private String lastHeartbeatAt;
    private String instanceCount;
    private String createdAt;
    private String updatedAt;

    public String getId() {
        return this.id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getClusterId() {
        return this.clusterId;
    }

    public void setClusterId(String clusterId) {
        this.clusterId = clusterId;
    }

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public String getHostname() {
        return this.hostname;
    }

    public void setHostname(String hostname) {
        this.hostname = hostname;
    }

    public String getMachineCode() {
        return this.machineCode;
    }

    public void setMachineCode(String machineCode) {
        this.machineCode = machineCode;
    }

    public String getOsName() {
        return this.osName;
    }

    public void setOsName(String osName) {
        this.osName = osName;
    }

    public String getOsVersion() {
        return this.osVersion;
    }

    public void setOsVersion(String osVersion) {
        this.osVersion = osVersion;
    }

    public String getKernelVersion() {
        return this.kernelVersion;
    }

    public void setKernelVersion(String kernelVersion) {
        this.kernelVersion = kernelVersion;
    }

    public String getArch() {
        return this.arch;
    }

    public void setArch(String arch) {
        this.arch = arch;
    }

    public String getCpuModel() {
        return this.cpuModel;
    }

    public void setCpuModel(String cpuModel) {
        this.cpuModel = cpuModel;
    }

    public Integer getCpuCores() {
        return this.cpuCores;
    }

    public void setCpuCores(Integer cpuCores) {
        this.cpuCores = cpuCores;
    }

    public String getMemoryTotalMb() {
        return this.memoryTotalMb;
    }

    public void setMemoryTotalMb(String memoryTotalMb) {
        this.memoryTotalMb = memoryTotalMb;
    }

    public String getRemoteIp() {
        return this.remoteIp;
    }

    public void setRemoteIp(String remoteIp) {
        this.remoteIp = remoteIp;
    }

    public List<String> getLocalIps() {
        return this.localIps;
    }

    public void setLocalIps(List<String> localIps) {
        this.localIps = localIps;
    }

    public List<String> getMacAddresses() {
        return this.macAddresses;
    }

    public void setMacAddresses(List<String> macAddresses) {
        this.macAddresses = macAddresses;
    }

    public String getDaemonVersion() {
        return this.daemonVersion;
    }

    public void setDaemonVersion(String daemonVersion) {
        this.daemonVersion = daemonVersion;
    }

    public Integer getStatus() {
        return this.status;
    }

    public void setStatus(Integer status) {
        this.status = status;
    }

    public String getLastHeartbeatAt() {
        return this.lastHeartbeatAt;
    }

    public void setLastHeartbeatAt(String lastHeartbeatAt) {
        this.lastHeartbeatAt = lastHeartbeatAt;
    }

    public String getInstanceCount() {
        return this.instanceCount;
    }

    public void setInstanceCount(String instanceCount) {
        this.instanceCount = instanceCount;
    }

    public String getCreatedAt() {
        return this.createdAt;
    }

    public void setCreatedAt(String createdAt) {
        this.createdAt = createdAt;
    }

    public String getUpdatedAt() {
        return this.updatedAt;
    }

    public void setUpdatedAt(String updatedAt) {
        this.updatedAt = updatedAt;
    }
}
