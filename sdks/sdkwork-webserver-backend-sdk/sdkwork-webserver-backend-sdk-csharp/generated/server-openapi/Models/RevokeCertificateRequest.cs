using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class RevokeCertificateRequest
    {
        public string Reason { get; set; }
    }
}
