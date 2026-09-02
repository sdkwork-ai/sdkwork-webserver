using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class MediaResource
    {
        public string? Id { get; set; }
        public string Kind { get; set; }
        public string Source { get; set; }
        public string? Url { get; set; }
        public string? PublicUrl { get; set; }
        public string? Uri { get; set; }
        public string? ObjectBlobId { get; set; }
        public string? FileName { get; set; }
        public string? MimeType { get; set; }
        public string? SizeBytes { get; set; }
        public MediaChecksum? Checksum { get; set; }
        public int? Width { get; set; }
        public int? Height { get; set; }
        public double? DurationSeconds { get; set; }
        public string? AltText { get; set; }
        public string? Title { get; set; }
        public Dictionary<string, object>? Metadata { get; set; }
    }
}
