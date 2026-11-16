using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CreateDomainVerificationFileRequest
    {
        public string Platform { get; set; }
        public string FileName { get; set; }
        public string Content { get; set; }
    }
}
