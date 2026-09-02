using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class AuditLogPage
    {
        public List<AuditLogResponse>? Items { get; set; }
        public string? Total { get; set; }
    }
}
