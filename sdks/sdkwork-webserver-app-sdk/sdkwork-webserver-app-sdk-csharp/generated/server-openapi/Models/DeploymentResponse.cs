using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class DeploymentResponse
    {
        public string Id { get; set; }
        public string ApplicationId { get; set; }
        public int DeployType { get; set; }
        public string? SourceVersionId { get; set; }
        public string? VersionTag { get; set; }
        public string? CommitHash { get; set; }
        public string? SourceRef { get; set; }
        public string? RollbackFromDeploymentId { get; set; }
        public string Environment { get; set; }
        public string? ArtifactDriveUri { get; set; }
        public string? ArtifactSize { get; set; }
        public string? ArtifactHash { get; set; }
        public int Status { get; set; }
        public string? StartedAt { get; set; }
        public string? CompletedAt { get; set; }
        public string? DurationMs { get; set; }
        public string CreatedAt { get; set; }
    }
}
