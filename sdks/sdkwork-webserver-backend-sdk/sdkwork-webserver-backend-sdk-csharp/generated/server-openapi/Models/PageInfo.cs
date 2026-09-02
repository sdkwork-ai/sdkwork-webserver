using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class PageInfo
    {
        public string Mode { get; set; }
        public int? Page { get; set; }
        public int? PageSize { get; set; }
        public string? TotalItems { get; set; }
        public int? TotalPages { get; set; }
        public string? NextCursor { get; set; }
        public bool? HasMore { get; set; }
    }
}
