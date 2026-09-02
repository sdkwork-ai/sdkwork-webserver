using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpdateNginxConfigRequest
    {
        public string? ConfigContent { get; set; }
        public string? ConfigName { get; set; }
    }
}
