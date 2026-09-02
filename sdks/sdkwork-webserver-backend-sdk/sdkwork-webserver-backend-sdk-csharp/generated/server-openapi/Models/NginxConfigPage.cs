using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class NginxConfigPage
    {
        public List<NginxConfigResponse>? Items { get; set; }
        public string? Total { get; set; }
    }
}
