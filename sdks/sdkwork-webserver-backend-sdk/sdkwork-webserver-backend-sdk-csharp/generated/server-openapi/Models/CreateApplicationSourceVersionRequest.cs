using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class CreateApplicationSourceVersionRequest
    {
        public string VersionTag { get; set; }
        public string SourceType { get; set; }
        public string? SourceRef { get; set; }
        public string? CommitHash { get; set; }
        public string ArtifactDriveUri { get; set; }
        public string ArtifactSize { get; set; }
        public string ArtifactHash { get; set; }
        public ApplicationSourceVersionConfigSnapshot? ConfigSnapshot { get; set; }
    }
}
