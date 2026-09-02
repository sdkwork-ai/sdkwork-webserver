using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CreateNginxConfigRequest
    {
        public int ConfigType { get; set; }
        public string ConfigName { get; set; }
        public string ConfigContent { get; set; }
        public string SiteId { get; set; }
    }
}
