namespace SDKWork.WebserverAppSdk.Api
{
    public static class ApiPaths
    {
        public const string ApiPrefix = "/app/v3/api";

        public static string AppPath(string path = "")
        {
            if (string.IsNullOrEmpty(path)) return ApiPrefix;
            if (path.StartsWith("http://") || path.StartsWith("https://")) return path;

            var normalizedPrefix = (ApiPrefix ?? string.Empty).Trim();
            if (!string.IsNullOrEmpty(normalizedPrefix) && normalizedPrefix != "/")
            {
                normalizedPrefix = "/" + normalizedPrefix.Trim('/');
            }
            else
            {
                normalizedPrefix = string.Empty;
            }

            var normalizedPath = path.StartsWith("/") ? path : "/" + path;
            if (string.IsNullOrEmpty(normalizedPrefix)) return normalizedPath;
            if (normalizedPath == normalizedPrefix || normalizedPath.StartsWith(normalizedPrefix + "/")) return normalizedPath;
            return normalizedPrefix + normalizedPath;
        }

        public static string AppendQueryString(string path, string rawQueryString)
        {
            var query = (rawQueryString ?? string.Empty).TrimStart('?');
            if (string.IsNullOrEmpty(query)) return path;
            return path.Contains("?") ? path + "&" + query : path + "?" + query;
        }
    }
}
