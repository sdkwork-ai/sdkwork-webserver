using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class DnsAccountResponse
    {
        public string AccountId { get; set; }
        public string Provider { get; set; }
        public string ZoneApex { get; set; }
    }
}
