using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class CreateServerRequest
    {
        public string Name { get; set; }
        public string Host { get; set; }
        public string TenantScopeHash { get; set; }
        public int SshPort { get; set; }
    }
}
