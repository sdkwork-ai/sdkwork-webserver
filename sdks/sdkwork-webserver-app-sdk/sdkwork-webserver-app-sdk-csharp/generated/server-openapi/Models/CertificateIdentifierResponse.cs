using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class CertificateIdentifierResponse
    {
        public string DomainId { get; set; }
        public string Hostname { get; set; }
        public string IdentifierType { get; set; }
        public int Position { get; set; }
    }
}
