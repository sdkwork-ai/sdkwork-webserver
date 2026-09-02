using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class AgentCertificateObservation
    {
        public string CertificateId { get; set; }
        public string Fingerprint { get; set; }
        public string SyncVersion { get; set; }
        public string State { get; set; }
        public string ObservedAt { get; set; }
        public string? FailureCode { get; set; }
    }
}
