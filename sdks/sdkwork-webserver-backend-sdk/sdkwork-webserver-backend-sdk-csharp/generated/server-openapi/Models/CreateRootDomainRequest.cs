using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class CreateRootDomainRequest
    {
        public string Hostname { get; set; }
    }
}
