using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class SourceVersionPage
    {
        public List<SourceVersionResponse>? Items { get; set; }
        public string? Total { get; set; }
    }
}
