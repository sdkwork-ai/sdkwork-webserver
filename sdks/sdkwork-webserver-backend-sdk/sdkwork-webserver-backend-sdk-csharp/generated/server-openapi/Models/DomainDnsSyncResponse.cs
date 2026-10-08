using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class DomainDnsSyncResponse
    {
        public string RecordCount { get; set; }
        public string SyncedAt { get; set; }
        public string ZoneApex { get; set; }
        public string DnsProvider { get; set; }
        public string CloudAccountId { get; set; }
    }
}
