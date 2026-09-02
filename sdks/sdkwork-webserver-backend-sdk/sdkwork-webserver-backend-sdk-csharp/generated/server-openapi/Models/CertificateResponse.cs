using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CertificateResponse
    {
        public string Id { get; set; }
        public string CertName { get; set; }
        public List<CertificateIdentifierResponse> Identifiers { get; set; }
        public int? CertType { get; set; }
        public string? Issuer { get; set; }
        public string? Fingerprint { get; set; }
        public string KeyAlgorithm { get; set; }
        public string? NotBefore { get; set; }
        public string? NotAfter { get; set; }
        public bool? AutoRenew { get; set; }
        public string? RenewalStatus { get; set; }
        public string Status { get; set; }
        public string CreatedAt { get; set; }
    }
}
