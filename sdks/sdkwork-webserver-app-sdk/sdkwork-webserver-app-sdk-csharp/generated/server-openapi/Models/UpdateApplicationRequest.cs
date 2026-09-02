using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class UpdateApplicationRequest
    {
        public string? Name { get; set; }
        public string? Description { get; set; }
        public Dictionary<string, object>? RuntimeConfig { get; set; }
        public ApplicationStoreListing? StoreListing { get; set; }
    }
}
