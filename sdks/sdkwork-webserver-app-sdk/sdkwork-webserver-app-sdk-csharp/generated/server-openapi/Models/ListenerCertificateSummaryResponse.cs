using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class ListenerCertificateSummaryResponse
    {
        public string CertName { get; set; }
        public List<CertificateIdentifierResponse> Identifiers { get; set; }
        public string? Issuer { get; set; }
        public string? Fingerprint { get; set; }
        public string? NotAfter { get; set; }
        public string Status { get; set; }
    }
}
