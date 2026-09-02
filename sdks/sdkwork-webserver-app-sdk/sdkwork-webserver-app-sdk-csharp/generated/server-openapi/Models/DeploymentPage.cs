using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class DeploymentPage
    {
        public List<DeploymentResponse>? Items { get; set; }
        public string? Total { get; set; }
    }
}
