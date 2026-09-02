using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class ServerEntry
    {
        public string Name { get; set; }
        public string Kind { get; set; }
        public string Path { get; set; }
        public string? Size { get; set; }
        public string? ProjectType { get; set; }
        public bool? IsProjectRoot { get; set; }
    }
}
