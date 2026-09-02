using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class IssueCertificateRequest
    {
        public List<string> DomainIds { get; set; }
        public int CertType { get; set; }
        public string? KeyAlgorithm { get; set; }
        public bool? AutoRenew { get; set; }
    }
}
