using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ApplicationSourceVersionConfigSnapshot
    {
        public string AppConfigPath { get; set; }
        public string DeploymentConfigPath { get; set; }
        public bool AppConfigDetected { get; set; }
        public bool DeploymentConfigDetected { get; set; }
    }
}
