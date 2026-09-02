using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class ServerDirectoryListing
    {
        public string NodeId { get; set; }
        public string Path { get; set; }
        public string ParentPath { get; set; }
        public List<ServerEntry> Entries { get; set; }
    }
}
