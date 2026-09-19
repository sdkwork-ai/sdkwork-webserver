using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterHostResponse
    {
        public string Id { get; set; }
        public string ClusterId { get; set; }
        public string Name { get; set; }
        public string Hostname { get; set; }
        public string MachineCode { get; set; }
        public string? OsName { get; set; }
        public string? OsVersion { get; set; }
        public string? KernelVersion { get; set; }
        public string? Arch { get; set; }
        public string? CpuModel { get; set; }
        public int? CpuCores { get; set; }
        public string? MemoryTotalMb { get; set; }
        public string? RemoteIp { get; set; }
        public List<string> LocalIps { get; set; }
        public List<string> MacAddresses { get; set; }
        public string? DaemonVersion { get; set; }
        public int Status { get; set; }
        public string? LastHeartbeatAt { get; set; }
        public string InstanceCount { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
