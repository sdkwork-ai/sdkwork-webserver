using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class ApplicationStoreListing
    {
        public MediaResource? Icon { get; set; }
        public MediaResource? Cover { get; set; }
        public List<MediaResource>? Previews { get; set; }
        public string? ShortDescription { get; set; }
        public string? FullDescription { get; set; }
        public string? ReleaseNotes { get; set; }
        public string? Category { get; set; }
        public List<string>? Keywords { get; set; }
        public string? SupportUrl { get; set; }
        public string? PrivacyPolicyUrl { get; set; }
        public string? OfficialWebsiteUrl { get; set; }
    }
}
