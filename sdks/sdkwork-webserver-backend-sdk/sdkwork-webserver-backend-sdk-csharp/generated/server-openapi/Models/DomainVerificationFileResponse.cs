using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class DomainVerificationFileResponse
    {
        public string Id { get; set; }
        public string RootDomainId { get; set; }
        public string Platform { get; set; }
        public string FileName { get; set; }
        public string Content { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
