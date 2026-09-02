using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class ImportApplicationGitSourceVersionRequest
    {
        public string VersionTag { get; set; }
        public string RepositoryUrl { get; set; }
        public string? GitRef { get; set; }
    }
}
