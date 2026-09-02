using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class ImportGitSourceVersionRequest
    {
        public string VersionTag { get; set; }
        public string RepositoryUrl { get; set; }
        public string? GitRef { get; set; }
    }
}
