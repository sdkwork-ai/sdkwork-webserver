using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class FieldError
    {
        public string Field { get; set; }
        public string Message { get; set; }
        public int? Code { get; set; }
    }
}
