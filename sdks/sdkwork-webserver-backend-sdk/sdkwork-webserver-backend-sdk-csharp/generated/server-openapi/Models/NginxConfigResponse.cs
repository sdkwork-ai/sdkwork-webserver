using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class NginxConfigResponse
    {
        public string? Id { get; set; }
        public int? ConfigType { get; set; }
        public string? ConfigName { get; set; }
        public string? ConfigContent { get; set; }
        public string? ConfigHash { get; set; }
        public bool? IsActive { get; set; }
        public int? VersionNo { get; set; }
        public string? DeployedAt { get; set; }
        public int? Status { get; set; }
        public string? CreatedAt { get; set; }
        public string? UpdatedAt { get; set; }
    }
}
