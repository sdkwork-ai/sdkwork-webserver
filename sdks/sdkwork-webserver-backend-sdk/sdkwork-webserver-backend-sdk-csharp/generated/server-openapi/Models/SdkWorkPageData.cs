using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class SdkWorkPageData
    {
        public List<Dictionary<string, object>> Items { get; set; }
        public PageInfo PageInfo { get; set; }
    }
}
