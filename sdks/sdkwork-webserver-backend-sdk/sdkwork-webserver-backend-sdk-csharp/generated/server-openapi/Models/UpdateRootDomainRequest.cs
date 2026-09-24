using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpdateRootDomainRequest
    {
        public string? DisplayName { get; set; }
        public string? DnsProvider { get; set; }
        public string? ProviderZoneRef { get; set; }
        public int? Status { get; set; }
    }
}
