using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ServerOperationResult
    {
        public string OperationId { get; set; }
        public int? ExitCode { get; set; }
        public string? Stdout { get; set; }
        public string? Stderr { get; set; }
    }
}
