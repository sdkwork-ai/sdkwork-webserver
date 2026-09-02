using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class CreateDomainRequest
    {
        public string Hostname { get; set; }
        public bool? IsPrimary { get; set; }
        public bool? SslEnabled { get; set; }
        public string? SslProvider { get; set; }
    }
}
