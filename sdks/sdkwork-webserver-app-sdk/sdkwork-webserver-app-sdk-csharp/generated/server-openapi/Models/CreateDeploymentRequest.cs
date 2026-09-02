using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class CreateDeploymentRequest
    {
        public string? SourceVersionId { get; set; }
        public int DeployType { get; set; }
        public string? VersionTag { get; set; }
        public string? CommitHash { get; set; }
        public string? SourceRef { get; set; }
        public string? ArtifactDriveUri { get; set; }
        public string? ArtifactSize { get; set; }
        public string? ArtifactHash { get; set; }
        public string? Environment { get; set; }
    }
}
