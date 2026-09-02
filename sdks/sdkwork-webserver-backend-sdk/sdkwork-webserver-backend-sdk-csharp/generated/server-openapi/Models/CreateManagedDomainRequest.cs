using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CreateManagedDomainRequest
    {
        public string Hostname { get; set; }
        public string? ApplicationId { get; set; }
        public bool? IsPrimary { get; set; }
        public bool? SslEnabled { get; set; }
        public string? SslProvider { get; set; }
    }
}
