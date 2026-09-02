using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.AppSdk.Models
{
    public class SourceVersionConfigSnapshot
    {
        public string AppConfigPath { get; set; }
        public string DeploymentConfigPath { get; set; }
        public bool AppConfigDetected { get; set; }
        public bool DeploymentConfigDetected { get; set; }
    }
}
