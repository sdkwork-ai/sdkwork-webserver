using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class DomainVerifyResponse
    {
        public bool Verified { get; set; }
        public string Status { get; set; }
        public string Method { get; set; }
        public string RecordName { get; set; }
        public string RecordValue { get; set; }
        public int AttemptCount { get; set; }
        public string ExpiresAt { get; set; }
        public string? NextAttemptAt { get; set; }
        public string? CheckedAt { get; set; }
        public string? FailureCode { get; set; }
    }
}
