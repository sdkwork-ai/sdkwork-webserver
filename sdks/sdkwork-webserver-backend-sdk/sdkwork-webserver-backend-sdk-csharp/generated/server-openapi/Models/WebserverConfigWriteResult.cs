using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class WebserverConfigWriteResult
    {
        public string Id { get; set; }
        public string Path { get; set; }
        public string Size { get; set; }
        public string Sha256 { get; set; }
        public string? BackupPath { get; set; }
        public string UpdatedAt { get; set; }
    }
}
