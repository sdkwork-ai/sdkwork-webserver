using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ServerFilesNode
    {
        public string Id { get; set; }
        public string Name { get; set; }
        public string Host { get; set; }
        public int SshPort { get; set; }
        public string Status { get; set; }
        public string FilesystemRoot { get; set; }
        public string? Region { get; set; }
    }
}
