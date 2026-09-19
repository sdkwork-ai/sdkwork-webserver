using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using SDKWork.WebserverBackendSdk.Models;
using SdkHttpClient = SDKWork.WebserverBackendSdk.Http.HttpClient;

namespace SDKWork.WebserverBackendSdk.Api
{
    public class ClusterApi
    {
        private readonly SdkHttpClient _client;

        public ClusterApi(SdkHttpClient client)
        {
            _client = client;
        }

        /// <summary>
        /// List Web Server clusters
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersListResponse?> ClustersListAsync(int? page = null, int? pageSize = null)
        {
            var queryString = BuildQueryString(new[]
            {
                new QueryParameterSpec("page", page, "form", true, false, null),
                new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            });
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersListResponse>(ApiPaths.AppendQueryString(ApiPaths.BackendPath("/clusters"), queryString));
        }

        /// <summary>
        /// Create a Web Server cluster
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersCreateResponse201?> ClustersCreateAsync(SDKWork.WebserverBackendSdk.Models.CreateClusterRequest body, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            return await _client.PostAsync<SDKWork.WebserverBackendSdk.Models.ClustersCreateResponse201>(ApiPaths.BackendPath("/clusters"), body, null, requestHeaders, "application/json");
        }

        /// <summary>
        /// Retrieve a Web Server cluster
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersRetrieveResponse?> ClustersRetrieveAsync(string clusterId)
        {
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersRetrieveResponse>(ApiPaths.BackendPath($"/clusters/{SerializePathParameter(clusterId, new PathParameterSpec("clusterId", "simple", false))}"));
        }

        /// <summary>
        /// Update a Web Server cluster
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersUpdateResponse?> ClustersUpdateAsync(string clusterId, SDKWork.WebserverBackendSdk.Models.UpdateClusterRequest body, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            return await _client.PatchAsync<SDKWork.WebserverBackendSdk.Models.ClustersUpdateResponse>(ApiPaths.BackendPath($"/clusters/{SerializePathParameter(clusterId, new PathParameterSpec("clusterId", "simple", false))}"), body, null, requestHeaders, "application/json");
        }

        /// <summary>
        /// Delete an empty Web Server cluster
        /// </summary>
        public async Task ClustersDeleteAsync(string clusterId, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            await _client.DeleteAsync<object>(ApiPaths.BackendPath($"/clusters/{SerializePathParameter(clusterId, new PathParameterSpec("clusterId", "simple", false))}"), null, requestHeaders);
        }

        /// <summary>
        /// List cluster hosts with system and network identity
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersHostsListResponse?> ClustersHostsListAsync(int? pageSize = null, string? cursor = null, string? clusterId = null, int? status = null)
        {
            var queryString = BuildQueryString(new[]
            {
                new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
                new QueryParameterSpec("cursor", cursor, "form", true, false, null),
                new QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
                new QueryParameterSpec("status", status, "form", true, false, null),
            });
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersHostsListResponse>(ApiPaths.AppendQueryString(ApiPaths.BackendPath("/clusters/hosts"), queryString));
        }

        /// <summary>
        /// Retrieve a cluster host
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersHostsRetrieveResponse?> ClustersHostsRetrieveAsync(string hostId)
        {
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersHostsRetrieveResponse>(ApiPaths.BackendPath($"/clusters/hosts/{SerializePathParameter(hostId, new PathParameterSpec("hostId", "simple", false))}"));
        }

        /// <summary>
        /// Rename a host or reassign it to another cluster
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersHostsUpdateResponse?> ClustersHostsUpdateAsync(string hostId, SDKWork.WebserverBackendSdk.Models.UpdateClusterHostRequest body, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            return await _client.PatchAsync<SDKWork.WebserverBackendSdk.Models.ClustersHostsUpdateResponse>(ApiPaths.BackendPath($"/clusters/hosts/{SerializePathParameter(hostId, new PathParameterSpec("hostId", "simple", false))}"), body, null, requestHeaders, "application/json");
        }

        /// <summary>
        /// Remove an instance-free host from the cluster inventory
        /// </summary>
        public async Task ClustersHostsDeleteAsync(string hostId, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            await _client.DeleteAsync<object>(ApiPaths.BackendPath($"/clusters/hosts/{SerializePathParameter(hostId, new PathParameterSpec("hostId", "simple", false))}"), null, requestHeaders);
        }

        /// <summary>
        /// List webserver process instances with liveness state
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersInstancesListResponse?> ClustersInstancesListAsync(int? pageSize = null, string? cursor = null, string? clusterId = null, string? hostId = null, int? status = null, string? healthState = null)
        {
            var queryString = BuildQueryString(new[]
            {
                new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
                new QueryParameterSpec("cursor", cursor, "form", true, false, null),
                new QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
                new QueryParameterSpec("host_id", hostId, "form", true, false, null),
                new QueryParameterSpec("status", status, "form", true, false, null),
                new QueryParameterSpec("health_state", healthState, "form", true, false, null),
            });
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersInstancesListResponse>(ApiPaths.AppendQueryString(ApiPaths.BackendPath("/clusters/instances"), queryString));
        }

        /// <summary>
        /// Retrieve a webserver process instance
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersInstancesRetrieveResponse?> ClustersInstancesRetrieveAsync(string instanceId)
        {
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersInstancesRetrieveResponse>(ApiPaths.BackendPath($"/clusters/instances/{SerializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false))}"));
        }

        /// <summary>
        /// Update an instance display name, status, or advertised endpoint
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersInstancesUpdateResponse?> ClustersInstancesUpdateAsync(string instanceId, SDKWork.WebserverBackendSdk.Models.UpdateClusterInstanceRequest body, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            return await _client.PatchAsync<SDKWork.WebserverBackendSdk.Models.ClustersInstancesUpdateResponse>(ApiPaths.BackendPath($"/clusters/instances/{SerializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false))}"), body, null, requestHeaders, "application/json");
        }

        /// <summary>
        /// Unregister a webserver process instance
        /// </summary>
        public async Task ClustersInstancesDeleteAsync(string instanceId, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            await _client.DeleteAsync<object>(ApiPaths.BackendPath($"/clusters/instances/{SerializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false))}"), null, requestHeaders);
        }

        /// <summary>
        /// List cluster lifecycle events
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersEventsListResponse?> ClustersEventsListAsync(int? pageSize = null, string? cursor = null, string? clusterId = null, string? severity = null)
        {
            var queryString = BuildQueryString(new[]
            {
                new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
                new QueryParameterSpec("cursor", cursor, "form", true, false, null),
                new QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
                new QueryParameterSpec("severity", severity, "form", true, false, null),
            });
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersEventsListResponse>(ApiPaths.AppendQueryString(ApiPaths.BackendPath("/clusters/events"), queryString));
        }

        /// <summary>
        /// Retrieve the cluster health overview for status polling
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersOverviewRetrieveResponse?> ClustersOverviewRetrieveAsync()
        {
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersOverviewRetrieveResponse>(ApiPaths.BackendPath("/clusters/overview"));
        }

        /// <summary>
        /// List one instance's stored heartbeat samples
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersInstancesHeartbeatsListResponse?> ClustersInstancesHeartbeatsListAsync(string instanceId, int? pageSize = null, string? cursor = null)
        {
            var queryString = BuildQueryString(new[]
            {
                new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
                new QueryParameterSpec("cursor", cursor, "form", true, false, null),
            });
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.ClustersInstancesHeartbeatsListResponse>(ApiPaths.AppendQueryString(ApiPaths.BackendPath($"/clusters/instances/{SerializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false))}/heartbeats"), queryString));
        }

        /// <summary>
        /// Enqueue a peer message to one instance or broadcast to online members
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.ClustersMessagesCreateResponse201?> ClustersMessagesCreateAsync(SDKWork.WebserverBackendSdk.Models.EnqueueClusterPeerMessagesRequest body, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            return await _client.PostAsync<SDKWork.WebserverBackendSdk.Models.ClustersMessagesCreateResponse201>(ApiPaths.BackendPath("/clusters/messages"), body, null, requestHeaders, "application/json");
        }

        private sealed record PathParameterSpec(string Name, string Style, bool Explode);

        private static string SerializePathParameter(object? value, PathParameterSpec spec)
        {
            if (value is null)
            {
                return string.Empty;
            }
            var style = string.IsNullOrWhiteSpace(spec.Style) ? "simple" : spec.Style;
            if (value is System.Collections.IDictionary dictionary)
            {
                return SerializePathObject(spec.Name, dictionary, style, spec.Explode);
            }
            if (value is System.Collections.IEnumerable enumerable && value is not string)
            {
                return SerializePathArray(spec.Name, enumerable, style, spec.Explode);
            }
            return PathPrimitivePrefix(spec.Name, style) + Uri.EscapeDataString(value.ToString() ?? string.Empty);
        }

        private static string SerializePathArray(string name, System.Collections.IEnumerable values, string style, bool explode)
        {
            var serialized = new List<string>();
            foreach (var item in values)
            {
                if (item is not null)
                {
                    serialized.Add(Uri.EscapeDataString(item.ToString() ?? string.Empty));
                }
            }
            if (serialized.Count == 0)
            {
                return PathPrefix(name, style);
            }
            if (style == "matrix")
            {
                if (explode)
                {
                    var parts = new List<string>();
                    foreach (var item in serialized)
                    {
                        parts.Add(";" + name + "=" + item);
                    }
                    return string.Join(string.Empty, parts);
                }
                return ";" + name + "=" + string.Join(",", serialized);
            }
            var separator = explode ? "." : ",";
            return PathPrefix(name, style) + string.Join(separator, serialized);
        }

        private static string SerializePathObject(string name, System.Collections.IDictionary values, string style, bool explode)
        {
            var entries = new List<string>();
            var exploded = new List<string>();
            foreach (System.Collections.DictionaryEntry item in values)
            {
                if (item.Value is null)
                {
                    continue;
                }
                var escapedKey = Uri.EscapeDataString(item.Key.ToString() ?? string.Empty);
                var escapedValue = Uri.EscapeDataString(item.Value.ToString() ?? string.Empty);
                if (explode)
                {
                    exploded.Add(style == "matrix" ? ";" + escapedKey + "=" + escapedValue : escapedKey + "=" + escapedValue);
                }
                else
                {
                    entries.Add(escapedKey);
                    entries.Add(escapedValue);
                }
            }
            if (style == "matrix")
            {
                return explode ? string.Join(string.Empty, exploded) : ";" + name + "=" + string.Join(",", entries);
            }
            if (explode)
            {
                var separator = style == "label" ? "." : ",";
                return PathPrefix(name, style) + string.Join(separator, exploded);
            }
            return PathPrefix(name, style) + string.Join(",", entries);
        }

        private static string PathPrefix(string name, string style)
        {
            return style switch
            {
                "label" => ".",
                "matrix" => ";" + name,
                _ => string.Empty,
            };
        }

        private static string PathPrimitivePrefix(string name, string style)
        {
            return style == "matrix" ? ";" + name + "=" : PathPrefix(name, style);
        }

        private sealed record QueryParameterSpec(
            string Name,
            object? Value,
            string Style,
            bool Explode,
            bool AllowReserved,
            string? ContentType);

        private static string BuildQueryString(IEnumerable<QueryParameterSpec> parameters)
        {
            var pairs = new List<string>();
            foreach (var parameter in parameters)
            {
                AppendSerializedParameter(pairs, parameter);
            }
            return string.Join("&", pairs);
        }

        private static void AppendSerializedParameter(List<string> pairs, QueryParameterSpec parameter)
        {
            if (parameter.Value is null)
            {
                return;
            }

            if (!string.IsNullOrWhiteSpace(parameter.ContentType))
            {
                var json = System.Text.Json.JsonSerializer.Serialize(parameter.Value);
                pairs.Add(Uri.EscapeDataString(parameter.Name) + "=" + EncodeQueryValue(json, parameter.AllowReserved));
                return;
            }

            var style = string.IsNullOrWhiteSpace(parameter.Style) ? "form" : parameter.Style;
            if (style == "deepObject" && parameter.Value is System.Collections.IDictionary deepObject)
            {
                AppendDeepObjectParameter(pairs, parameter.Name, deepObject, parameter.AllowReserved);
            }
            else if (parameter.Value is System.Collections.IEnumerable enumerable && parameter.Value is not string && parameter.Value is not System.Collections.IDictionary)
            {
                AppendArrayParameter(pairs, parameter.Name, enumerable, style, parameter.Explode, parameter.AllowReserved);
            }
            else if (parameter.Value is System.Collections.IDictionary dictionary)
            {
                AppendObjectParameter(pairs, parameter.Name, dictionary, style, parameter.Explode, parameter.AllowReserved);
            }
            else
            {
                pairs.Add(Uri.EscapeDataString(parameter.Name) + "=" + EncodeQueryValue(parameter.Value.ToString() ?? string.Empty, parameter.AllowReserved));
            }
        }

        private static void AppendArrayParameter(List<string> pairs, string name, System.Collections.IEnumerable values, string style, bool explode, bool allowReserved)
        {
            var serialized = new List<string>();
            foreach (var item in values)
            {
                if (item is not null)
                {
                    serialized.Add(item.ToString() ?? string.Empty);
                }
            }
            if (serialized.Count == 0)
            {
                return;
            }
            if (style == "form" && explode)
            {
                foreach (var item in serialized)
                {
                    pairs.Add(Uri.EscapeDataString(name) + "=" + EncodeQueryValue(item, allowReserved));
                }
                return;
            }
            pairs.Add(Uri.EscapeDataString(name) + "=" + EncodeQueryValue(string.Join(",", serialized), allowReserved));
        }

        private static void AppendObjectParameter(List<string> pairs, string name, System.Collections.IDictionary values, string style, bool explode, bool allowReserved)
        {
            var serialized = new List<string>();
            foreach (System.Collections.DictionaryEntry item in values)
            {
                if (item.Value is null)
                {
                    continue;
                }
                if (style == "form" && explode)
                {
                    pairs.Add(Uri.EscapeDataString(item.Key.ToString() ?? string.Empty) + "=" + EncodeQueryValue(item.Value.ToString() ?? string.Empty, allowReserved));
                }
                else
                {
                    serialized.Add(item.Key.ToString() ?? string.Empty);
                    serialized.Add(item.Value.ToString() ?? string.Empty);
                }
            }
            if (serialized.Count > 0)
            {
                pairs.Add(Uri.EscapeDataString(name) + "=" + EncodeQueryValue(string.Join(",", serialized), allowReserved));
            }
        }

        private static void AppendDeepObjectParameter(List<string> pairs, string name, System.Collections.IDictionary values, bool allowReserved)
        {
            foreach (System.Collections.DictionaryEntry item in values)
            {
                if (item.Value is not null)
                {
                    pairs.Add(Uri.EscapeDataString(name + "[" + item.Key + "]") + "=" + EncodeQueryValue(item.Value.ToString() ?? string.Empty, allowReserved));
                }
            }
        }

        private static string EncodeQueryValue(string value, bool allowReserved)
        {
            var encoded = Uri.EscapeDataString(value);
            if (!allowReserved)
            {
                return encoded;
            }
            return encoded
                .Replace("%3A", ":").Replace("%2F", "/").Replace("%3F", "?").Replace("%23", "#")
                .Replace("%5B", "[").Replace("%5D", "]").Replace("%40", "@").Replace("%21", "!")
                .Replace("%24", "$").Replace("%26", "&").Replace("%27", "'").Replace("%28", "(")
                .Replace("%29", ")").Replace("%2A", "*").Replace("%2B", "+").Replace("%2C", ",")
                .Replace("%3B", ";").Replace("%3D", "=");
        }

        private sealed record HeaderParameterSpec(object? Value, string Style, bool Explode, string? ContentType);

        private static Dictionary<string, string>? BuildRequestHeaders(
            Dictionary<string, HeaderParameterSpec> headers,
            Dictionary<string, HeaderParameterSpec> cookies)
        {
            var requestHeaders = new Dictionary<string, string>();
            foreach (var item in headers)
            {
                var serialized = SerializeParameterValue(item.Value);
                if (serialized is not null)
                {
                    requestHeaders[item.Key] = serialized;
                }
            }

            var cookieHeader = BuildCookieHeader(cookies);
            if (!string.IsNullOrEmpty(cookieHeader))
            {
                requestHeaders["Cookie"] = requestHeaders.TryGetValue("Cookie", out var existing) && !string.IsNullOrEmpty(existing)
                    ? existing + "; " + cookieHeader
                    : cookieHeader;
            }

            return requestHeaders.Count == 0 ? null : requestHeaders;
        }

        private static string BuildCookieHeader(Dictionary<string, HeaderParameterSpec> cookies)
        {
            var pairs = new List<string>();
            foreach (var item in cookies)
            {
                var serialized = SerializeParameterValue(item.Value);
                if (serialized is not null)
                {
                    pairs.Add(Uri.EscapeDataString(item.Key) + "=" + Uri.EscapeDataString(serialized));
                }
            }
            return string.Join("; ", pairs);
        }

        private static string? SerializeParameterValue(HeaderParameterSpec? parameter)
        {
            var value = parameter?.Value;
            if (value is null)
            {
                return null;
            }
            if (!string.IsNullOrWhiteSpace(parameter!.ContentType))
            {
                return System.Text.Json.JsonSerializer.Serialize(value);
            }
            if (value is System.Collections.IEnumerable enumerable && value is not string)
            {
                var values = new List<string>();
                foreach (var item in enumerable)
                {
                    if (item is not null)
                    {
                        values.Add(item.ToString() ?? string.Empty);
                    }
                }
                return string.Join(",", values);
            }
            if (value is System.Collections.IDictionary dictionary)
            {
                var values = new List<string>();
                foreach (System.Collections.DictionaryEntry item in dictionary)
                {
                    if (item.Value is null)
                    {
                        continue;
                    }
                    if (parameter.Explode)
                    {
                        values.Add((item.Key.ToString() ?? string.Empty) + "=" + (item.Value.ToString() ?? string.Empty));
                    }
                    else
                    {
                        values.Add(item.Key.ToString() ?? string.Empty);
                        values.Add(item.Value.ToString() ?? string.Empty);
                    }
                }
                return string.Join(",", values);
            }
            return value.ToString();
        }
    }
}
