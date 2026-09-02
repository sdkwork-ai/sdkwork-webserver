using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class AuditLogResponse
    {
        public string? Id { get; set; }
        public string? OperatorId { get; set; }
        public string? OperatorType { get; set; }
        public string? Action { get; set; }
        public string? TargetType { get; set; }
        public string? TargetId { get; set; }
        public string? TargetUuid { get; set; }
        public string? IpAddress { get; set; }
        public Dictionary<string, object>? Changes { get; set; }
        public string? CreatedAt { get; set; }
    }
}
