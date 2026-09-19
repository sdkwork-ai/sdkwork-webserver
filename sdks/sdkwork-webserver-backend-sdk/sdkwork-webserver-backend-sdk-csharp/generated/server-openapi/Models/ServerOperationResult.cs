using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ServerOperationResult
    {
        public string OperationId { get; set; }
        public int? ExitCode { get; set; }
        public bool? TimedOut { get; set; }
        public string? Stdout { get; set; }
        public string? Stderr { get; set; }
        public bool? StdoutTruncated { get; set; }
        public bool? StderrTruncated { get; set; }
        public int? Pid { get; set; }
        public string? PidFile { get; set; }
        public string? LogFile { get; set; }
        public bool? Stopped { get; set; }
        public string? Message { get; set; }
    }
}
