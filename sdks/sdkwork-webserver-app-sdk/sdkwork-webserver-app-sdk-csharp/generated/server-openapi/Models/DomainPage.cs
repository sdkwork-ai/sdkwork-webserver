using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class DomainPage
    {
        public List<DomainResponse>? Items { get; set; }
        public string? Total { get; set; }
    }
}
