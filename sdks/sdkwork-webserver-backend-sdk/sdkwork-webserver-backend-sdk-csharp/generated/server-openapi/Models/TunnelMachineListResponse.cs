using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TunnelMachineListResponse
    {
        public List<TunnelMachineResponse> Machines { get; set; }
        public PageInfo? PageInfo { get; set; }
    }
}
