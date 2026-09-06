using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class WebserverConfigEntry
    {
        public string Id { get; set; }
        public string Kind { get; set; }
        public string Name { get; set; }
        public string Path { get; set; }
        public string Language { get; set; }
        public string? Size { get; set; }
        public string? UpdatedAt { get; set; }
        public bool Writable { get; set; }
    }
}
