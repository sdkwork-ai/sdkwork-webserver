using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpdateDomainApplicationBindingRequest
    {
        public string ApplicationId { get; set; }
        public bool? IsPrimary { get; set; }
    }
}
