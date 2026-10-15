package com.sdkwork.webserver.backend.sdk.model;

import java.util.List;

public class TunnelMachineListResponse {
    private List<TunnelMachineResponse> machines;

    public List<TunnelMachineResponse> getMachines() {
        return this.machines;
    }

    public void setMachines(List<TunnelMachineResponse> machines) {
        this.machines = machines;
    }
}
