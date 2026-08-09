using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Web.AppSdk.Models
{
    public class ListenerCertificateBindingResponse
    {
        public string Id { get; set; }
        public string ApplicationId { get; set; }
        public string DomainId { get; set; }
        public string CertificateId { get; set; }
        public string DesiredCertificateVersionId { get; set; }
        public string? CurrentCertificateVersionId { get; set; }
        public ListenerCertificateSummaryResponse DesiredCertificate { get; set; }
        public ListenerCertificateSummaryResponse? CurrentCertificate { get; set; }
        public string KeyAlgorithm { get; set; }
        public int Priority { get; set; }
        public bool IsDefault { get; set; }
        public string Status { get; set; }
        public string? ActivatedAt { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
