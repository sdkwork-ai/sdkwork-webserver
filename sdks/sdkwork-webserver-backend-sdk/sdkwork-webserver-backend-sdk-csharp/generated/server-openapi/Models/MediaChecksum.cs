using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class MediaChecksum
    {
        public string Algorithm { get; set; }
        public string Value { get; set; }
    }
}
