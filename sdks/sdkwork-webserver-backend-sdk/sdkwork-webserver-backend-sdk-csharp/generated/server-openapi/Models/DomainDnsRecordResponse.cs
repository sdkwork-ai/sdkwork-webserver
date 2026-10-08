using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class DomainDnsRecordResponse
    {
        public string Id { get; set; }
        public string RecordName { get; set; }
        public string Host { get; set; }
        public string RecordType { get; set; }
        public string RecordValue { get; set; }
        public int? TtlSeconds { get; set; }
        public int? Priority { get; set; }
        public string? RecordLine { get; set; }
        public string RecordStatus { get; set; }
        public string? DomainId { get; set; }
        public string DnsProvider { get; set; }
        public string CloudAccountId { get; set; }
        public string? ProviderRecordRef { get; set; }
        public string SyncedAt { get; set; }
    }
}
