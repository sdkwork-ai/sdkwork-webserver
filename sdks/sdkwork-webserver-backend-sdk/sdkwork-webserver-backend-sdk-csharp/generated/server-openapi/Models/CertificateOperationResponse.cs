using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CertificateOperationResponse
    {
        public string Id { get; set; }
        public string CertificateId { get; set; }
        public string OperationType { get; set; }
        public string Status { get; set; }
        public int AttemptCount { get; set; }
        public int MaxAttempts { get; set; }
        public string NextAttemptAt { get; set; }
        public string? FailureCode { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
        public string? CompletedAt { get; set; }
    }
}
