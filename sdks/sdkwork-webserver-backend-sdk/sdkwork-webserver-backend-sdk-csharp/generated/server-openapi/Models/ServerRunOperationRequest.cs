using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class ServerRunOperationRequest
    {
        public string Path { get; set; }
        public string OperationId { get; set; }
    }
}
