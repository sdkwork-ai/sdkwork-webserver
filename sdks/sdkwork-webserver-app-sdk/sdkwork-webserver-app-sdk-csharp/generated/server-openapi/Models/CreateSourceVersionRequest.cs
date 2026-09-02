using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class CreateSourceVersionRequest
    {
        public string VersionTag { get; set; }
        public string SourceType { get; set; }
        public string? SourceRef { get; set; }
        public string? CommitHash { get; set; }
        public string ArtifactDriveUri { get; set; }
        public string ArtifactSize { get; set; }
        public string ArtifactHash { get; set; }
        public SourceVersionConfigSnapshot? ConfigSnapshot { get; set; }
    }
}
