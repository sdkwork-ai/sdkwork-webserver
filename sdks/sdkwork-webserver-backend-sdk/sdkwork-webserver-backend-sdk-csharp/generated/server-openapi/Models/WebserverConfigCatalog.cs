using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class WebserverConfigCatalog
    {
        public string ConfigRoot { get; set; }
        public List<WebserverConfigEntry> Items { get; set; }
        public bool? Truncated { get; set; }
        public PageInfo? PageInfo { get; set; }
    }
}
