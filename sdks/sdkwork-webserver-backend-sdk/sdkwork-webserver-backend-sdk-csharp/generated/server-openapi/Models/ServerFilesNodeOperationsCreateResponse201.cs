using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ServerFilesNodeOperationsCreateResponse201
    {
        public int Code { get; set; }
        public object Data { get; set; }
        public string TraceId { get; set; }
    }
}
