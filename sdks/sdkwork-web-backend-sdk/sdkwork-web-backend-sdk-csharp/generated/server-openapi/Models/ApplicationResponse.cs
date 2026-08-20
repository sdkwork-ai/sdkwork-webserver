using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Web.BackendSdk.Models
{
    public class ApplicationResponse
    {
        public string Id { get; set; }
        public string Name { get; set; }
        public string Slug { get; set; }
        public string? Description { get; set; }
        public string? AppKind { get; set; }
        public int SiteType { get; set; }
        public int Status { get; set; }
        public Dictionary<string, object>? RuntimeConfig { get; set; }
        public ApplicationStoreListing? StoreListing { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
