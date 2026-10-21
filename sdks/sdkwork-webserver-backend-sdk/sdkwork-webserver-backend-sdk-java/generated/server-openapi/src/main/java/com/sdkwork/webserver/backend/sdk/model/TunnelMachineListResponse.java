package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class TunnelMachineListResponse {
    private List<TunnelMachineResponse> machines;
    private PageInfo pageInfo;

    public List<TunnelMachineResponse> getMachines() {
        return this.machines;
    }

    public void setMachines(List<TunnelMachineResponse> machines) {
        this.machines = machines;
    }

    public PageInfo getPageInfo() {
        return this.pageInfo;
    }

    public void setPageInfo(PageInfo pageInfo) {
        this.pageInfo = pageInfo;
    }
}
