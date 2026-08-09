using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Web.AppSdk.Models
{
    public class ApplicationPage
    {
        public List<ApplicationResponse>? Items { get; set; }
        public string? Total { get; set; }
        public int? Page { get; set; }
        public int? PageSize { get; set; }
    }
}
