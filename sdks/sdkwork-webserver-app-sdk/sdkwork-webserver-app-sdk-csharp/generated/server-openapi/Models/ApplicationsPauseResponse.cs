using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class ApplicationsPauseResponse
    {
        public int Code { get; set; }
        public object Data { get; set; }
        public string TraceId { get; set; }
    }
}
