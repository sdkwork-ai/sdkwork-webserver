using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CreateDomainDnsRecordRequest
    {
        public string RecordType { get; set; }
        public string Host { get; set; }
        public string RecordValue { get; set; }
        public int? TtlSeconds { get; set; }
        public int? Priority { get; set; }
        public string? RecordLine { get; set; }
    }
}
