using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class CreateListenerCertificateBindingRequest
    {
        public string CertificateId { get; set; }
        public string? CertificateVersionId { get; set; }
        public int? Priority { get; set; }
        public bool? IsDefault { get; set; }
    }
}
