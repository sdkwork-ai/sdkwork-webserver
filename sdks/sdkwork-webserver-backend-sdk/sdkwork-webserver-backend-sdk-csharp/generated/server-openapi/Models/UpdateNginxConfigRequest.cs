using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class UpdateNginxConfigRequest
    {
        public string? ConfigContent { get; set; }
        public string? ConfigName { get; set; }
    }
}
