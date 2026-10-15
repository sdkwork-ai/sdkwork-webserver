using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using SDKWork.WebserverBackendSdk.Models;
using SdkHttpClient = SDKWork.WebserverBackendSdk.Http.HttpClient;

namespace SDKWork.WebserverBackendSdk.Api
{
    public class TunnelApi
    {
        private readonly SdkHttpClient _client;

        public TunnelApi(SdkHttpClient client)
        {
            _client = client;
        }

        /// <summary>
        /// List the tunnel connector machines this gateway manages
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.TunnelMachinesListResponse?> MachinesListAsync()
        {
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.TunnelMachinesListResponse>(ApiPaths.BackendPath("/tunnel_machines"));
        }

        /// <summary>
        /// Retrieve one managed tunnel machine
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.TunnelMachinesRetrieveResponse?> MachinesRetrieveAsync(string deviceId)
        {
            return await _client.GetAsync<SDKWork.WebserverBackendSdk.Models.TunnelMachinesRetrieveResponse>(ApiPaths.BackendPath($"/tunnel_machines/{SerializePathParameter(deviceId, new PathParameterSpec("deviceId", "simple", false))}"));
        }

        /// <summary>
        /// Switch one tunnel machine on or off
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.TunnelMachinesEnabledUpdateResponse?> MachinesEnabledUpdateAsync(string deviceId, SDKWork.WebserverBackendSdk.Models.SetTunnelMachineEnabledRequest body)
        {
            return await _client.PutAsync<SDKWork.WebserverBackendSdk.Models.TunnelMachinesEnabledUpdateResponse>(ApiPaths.BackendPath($"/tunnel_machines/{SerializePathParameter(deviceId, new PathParameterSpec("deviceId", "simple", false))}/enabled"), body, null, null, "application/json");
        }

        /// <summary>
        /// Declare a route for one tunnel machine
        /// </summary>
        public async Task<SDKWork.WebserverBackendSdk.Models.TunnelMachinesRoutesCreateResponse201?> MachinesRoutesCreateAsync(string deviceId, SDKWork.WebserverBackendSdk.Models.CreateTunnelMachineRouteRequest body, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            return await _client.PostAsync<SDKWork.WebserverBackendSdk.Models.TunnelMachinesRoutesCreateResponse201>(ApiPaths.BackendPath($"/tunnel_machines/{SerializePathParameter(deviceId, new PathParameterSpec("deviceId", "simple", false))}/routes"), body, null, requestHeaders, "application/json");
        }

        /// <summary>
        /// Remove one route from a tunnel machine
        /// </summary>
        public async Task MachinesRoutesDeleteAsync(string deviceId, string routeId, string idempotencyKey)
        {
            var requestHeaders = BuildRequestHeaders(
                new Dictionary<string, HeaderParameterSpec>
                {
                    ["Idempotency-Key"] = new HeaderParameterSpec(idempotencyKey, "simple", false, null),
                },
                new Dictionary<string, HeaderParameterSpec>()
            );
            await _client.DeleteAsync<object>(ApiPaths.BackendPath($"/tunnel_machines/{SerializePathParameter(deviceId, new PathParameterSpec("deviceId", "simple", false))}/routes/{SerializePathParameter(routeId, new PathParameterSpec("routeId", "simple", false))}"), null, requestHeaders);
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
