using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class EnvVariablePage
    {
        public List<EnvVariableResponse>? Items { get; set; }
        public string? Total { get; set; }
    }
}
