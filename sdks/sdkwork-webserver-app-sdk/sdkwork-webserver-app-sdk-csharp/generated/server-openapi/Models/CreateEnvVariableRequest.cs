using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class CreateEnvVariableRequest
    {
        public string Key { get; set; }
        public string Value { get; set; }
        public string? Environment { get; set; }
        public bool? IsSecret { get; set; }
    }
}
