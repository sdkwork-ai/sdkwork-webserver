using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class ServerFileContent
    {
        public string NodeId { get; set; }
        public string Path { get; set; }
        public string Content { get; set; }
        public string Size { get; set; }
    }
}
