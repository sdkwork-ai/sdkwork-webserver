using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Net.Http;
using System.Net.Http.Json;
using System.Net.Http.Headers;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using SDKwork.Common.Core;

namespace SDKWork.Webserver.BackendSdk.Http
{
    public class HttpClient
    {

        private readonly System.Net.Http.HttpClient _client;
        private readonly string _baseUrl;

        public HttpClient(string baseUrl)
            : this(new SdkConfig(baseUrl))
        {
        }

        public HttpClient(SdkConfig config)
        {
            _baseUrl = config.BaseUrl.TrimEnd('/');
            _client = new System.Net.Http.HttpClient
            {
                Timeout = TimeSpan.FromMilliseconds(config.Timeout ?? DefaultValues.DEFAULT_TIMEOUT)
            };

            if (config.Headers != null)
            {
                foreach (var header in config.Headers)
                {
                    SetHeader(header.Key, header.Value);
                }
            }
        }
        public void SetAuthToken(string token)
        {
            _client.DefaultRequestHeaders.Authorization =
                new System.Net.Http.Headers.AuthenticationHeaderValue("Bearer", token);
        }
        public void SetAccessToken(string token)
        {
            if (_client.DefaultRequestHeaders.Contains("Access-Token"))
            {
                _client.DefaultRequestHeaders.Remove("Access-Token");
            }
            _client.DefaultRequestHeaders.TryAddWithoutValidation("Access-Token", token);
        }

        public void SetHeader(string key, string value)
        {
            if (_client.DefaultRequestHeaders.Contains(key))
            {
                _client.DefaultRequestHeaders.Remove(key);
            }
            _client.DefaultRequestHeaders.TryAddWithoutValidation(key, value);
        }

        private HttpRequestMessage BuildRequest(
            System.Net.Http.HttpMethod method,
            string path,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            HttpContent? content = null)
        {
            var request = new HttpRequestMessage(method, BuildUrl(path, parameters));
            if (content != null)
            {
                request.Content = content;
            }

            if (requestHeaders != null)
            {
                foreach (var header in requestHeaders)
                {
                    if (string.Equals(header.Key, "Content-Type", StringComparison.OrdinalIgnoreCase))
                    {
                        if (request.Content != null && !string.IsNullOrWhiteSpace(header.Value))
                        {
                            request.Content.Headers.ContentType = MediaTypeHeaderValue.Parse(header.Value);
                        }
                        continue;
                    }

                    request.Headers.Remove(header.Key);
                    request.Headers.TryAddWithoutValidation(header.Key, header.Value);
                }
            }

            return request;
        }

        private async Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            bool skipAuth = false,
            bool accessTokenOnly = false)
        {
            if (!skipAuth && !accessTokenOnly)
            {
                return await _client.SendAsync(request);
            }

            StripCredentialHeaders(request);
            if (accessTokenOnly)
            {
                var accessToken = _client.DefaultRequestHeaders
                    .FirstOrDefault(header => string.Equals(header.Key, "Access-Token", StringComparison.OrdinalIgnoreCase))
                    .Value?
                    .FirstOrDefault(value => !string.IsNullOrWhiteSpace(value))?
                    .Trim();
                if (string.IsNullOrWhiteSpace(accessToken))
                {
                    throw new InvalidOperationException(
                        "access-token-only request requires Access-Token before request dispatch");
                }
                request.Headers.TryAddWithoutValidation("Access-Token", accessToken);
            }

            using var anonymousClient = new System.Net.Http.HttpClient
            {
                Timeout = _client.Timeout
            };
            return await anonymousClient.SendAsync(request);
        }

        private static void StripCredentialHeaders(HttpRequestMessage request)
        {
            foreach (var header in new[]
            {
                "Authorization", "Access-Token", "X-API-Key", "X-Tenant-Id",
                "X-Organization-Id", "X-Platform", "X-User-Id",
                "X-Sdkwork-Tenant-Id", "X-Sdkwork-Organization-Id", "X-Sdkwork-User-Id"
            })
            {
                request.Headers.Remove(header);
            }
        }

        private static HttpContent CreateMultipartContent(object? body)
        {
            if (body is HttpContent rawContent)
            {
                return rawContent;
            }

            var multipart = new MultipartFormDataContent();
            void AddValue(string key, object? value)
            {
                if (value == null)
                {
                    multipart.Add(new StringContent(string.Empty), key);
                    return;
                }

                if (value is byte[] bytes)
                {
                    multipart.Add(new ByteArrayContent(bytes), key, key);
                    return;
                }

                if (value is IEnumerable values && value is not string && value is not byte[])
                {
                    foreach (var item in values)
                    {
                        AddValue(key, item);
                    }
                    return;
                }

                multipart.Add(new StringContent(Convert.ToString(value) ?? string.Empty), key);
            }

            switch (body)
            {
                case Dictionary<string, object> objectMap:
                    foreach (var pair in objectMap)
                    {
                        AddValue(pair.Key, pair.Value);
                    }
                    break;
                case Dictionary<string, string> stringMap:
                    foreach (var pair in stringMap)
                    {
                        AddValue(pair.Key, pair.Value);
                    }
                    break;
                default:
                    AddValue("value", body);
                    break;
            }

            return multipart;
        }

        private static HttpContent CreateFormContent(object? body)
        {
            var entries = new List<KeyValuePair<string, string>>();
            void AddEntry(string key, object? value)
            {
                if (value is IEnumerable values && value is not string && value is not byte[])
                {
                    foreach (var item in values)
                    {
                        AddEntry(key, item);
                    }
                    return;
                }

                entries.Add(new KeyValuePair<string, string>(key, Convert.ToString(value) ?? string.Empty));
            }

            switch (body)
            {
                case Dictionary<string, object> objectMap:
                    foreach (var pair in objectMap)
                    {
                        AddEntry(pair.Key, pair.Value);
                    }
                    break;
                case Dictionary<string, string> stringMap:
                    foreach (var pair in stringMap)
                    {
                        AddEntry(pair.Key, pair.Value);
                    }
                    break;
                default:
                    if (body != null)
                    {
                        AddEntry("value", body);
                    }
                    break;
            }

            return new FormUrlEncodedContent(entries);
        }

        private static HttpContent? CreateContent(object? body, string? contentType = null)
        {
            if (body == null)
            {
                return null;
            }

            if (body is HttpContent rawContent)
            {
                return rawContent;
            }

            var normalized = (contentType ?? "application/json").Trim().ToLowerInvariant();
            if (normalized.StartsWith("multipart/form-data"))
            {
                return CreateMultipartContent(body);
            }
            if (normalized.StartsWith("application/x-www-form-urlencoded"))
            {
                return CreateFormContent(body);
            }

            if (body is string text && !normalized.Contains("json"))
            {
                return new StringContent(text, Encoding.UTF8, contentType ?? "text/plain; charset=utf-8");
            }

            var json = JsonSerializer.Serialize(body);
            return new StringContent(json, Encoding.UTF8, "application/json");
        }

        private string BuildUrl(string path, Dictionary<string, object>? parameters = null)
        {
            var url = _baseUrl + path;
            if (parameters == null || parameters.Count == 0)
            {
                return url;
            }

            var query = string.Join("&", parameters.Select(p =>
                $"{Uri.EscapeDataString(p.Key)}={Uri.EscapeDataString(Convert.ToString(p.Value) ?? string.Empty)}"));
            return $"{url}?{query}";
        }

        private static async Task<T?> ReadResponseAsync<T>(HttpResponseMessage response)
        {
            response.EnsureSuccessStatusCode();

            if (response.Content == null || response.Content.Headers.ContentLength == 0)
            {
                return default;
            }

            var contentType = response.Content.Headers.ContentType?.MediaType ?? string.Empty;
            if (!contentType.Contains("application/json", StringComparison.OrdinalIgnoreCase))
            {
                return default;
            }

            return await response.Content.ReadFromJsonAsync<T>();
        }

        public async Task<T?> GetAsync<T>(
            string path,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            bool skipAuth = false)
        {
            using var request = BuildRequest(System.Net.Http.HttpMethod.Get, path, parameters, requestHeaders);
            var response = await SendAsync(request, skipAuth);
            return await ReadResponseAsync<T>(response);
        }

        public async Task<T?> RequestAsync<T>(
            string method,
            string path,
            object? body = null,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            string? contentType = null,
            bool skipAuth = false,
            bool accessTokenOnly = false)
        {
            using var content = CreateContent(body, contentType);
            using var request = BuildRequest(new System.Net.Http.HttpMethod(method), path, parameters, requestHeaders, content);
            var response = await SendAsync(request, skipAuth, accessTokenOnly);
            return await ReadResponseAsync<T>(response);
        }

        public async Task<T?> PostAsync<T>(
            string path,
            object? body = null,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            string? contentType = null,
            bool skipAuth = false,
            bool accessTokenOnly = false)
        {
            using var content = CreateContent(body, contentType);
            using var request = BuildRequest(System.Net.Http.HttpMethod.Post, path, parameters, requestHeaders, content);
            var response = await SendAsync(request, skipAuth, accessTokenOnly);
            return await ReadResponseAsync<T>(response);
        }

        public async IAsyncEnumerable<T> StreamAsync<T>(
            string method,
            string path,
            object? body = null,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            string? contentType = null,
            bool skipAuth = false,
            bool accessTokenOnly = false)
        {
            using var content = CreateContent(body, contentType);
            using var request = BuildRequest(new System.Net.Http.HttpMethod(method), path, parameters, requestHeaders, content);
            request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("text/event-stream"));
            var suppressStoredCredentials = skipAuth || accessTokenOnly;
            if (suppressStoredCredentials)
            {
                StripCredentialHeaders(request);
            }
            if (accessTokenOnly)
            {
                var accessToken = _client.DefaultRequestHeaders
                    .FirstOrDefault(header => string.Equals(header.Key, "Access-Token", StringComparison.OrdinalIgnoreCase))
                    .Value?
                    .FirstOrDefault(value => !string.IsNullOrWhiteSpace(value))?
                    .Trim();
                if (string.IsNullOrWhiteSpace(accessToken))
                {
                    throw new InvalidOperationException(
                        "access-token-only request requires Access-Token before request dispatch");
                }
                request.Headers.TryAddWithoutValidation("Access-Token", accessToken);
            }
            using var anonymousClient = suppressStoredCredentials
                ? new System.Net.Http.HttpClient { Timeout = _client.Timeout }
                : null;
            using var response = suppressStoredCredentials
                ? await anonymousClient!.SendAsync(request, HttpCompletionOption.ResponseHeadersRead)
                : await _client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead);
            response.EnsureSuccessStatusCode();
            await using var responseStream = await response.Content.ReadAsStreamAsync();
            using var reader = new StreamReader(responseStream);
            while (!reader.EndOfStream)
            {
                var line = await reader.ReadLineAsync();
                if (string.IsNullOrWhiteSpace(line) || line.StartsWith(":") || !line.StartsWith("data:"))
                {
                    continue;
                }
                var data = line.Substring(5).Trim();
                if (data == "[DONE]")
                {
                    yield break;
                }
                var item = JsonSerializer.Deserialize<T>(data);
                if (item != null)
                {
                    yield return item;
                }
            }
        }

        public async Task<T?> PutAsync<T>(
            string path,
            object? body = null,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            string? contentType = null,
            bool skipAuth = false)
        {
            using var content = CreateContent(body, contentType);
            using var request = BuildRequest(System.Net.Http.HttpMethod.Put, path, parameters, requestHeaders, content);
            var response = await SendAsync(request, skipAuth);
            return await ReadResponseAsync<T>(response);
        }

        public async Task<T?> DeleteAsync<T>(
            string path,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            bool skipAuth = false)
        {
            using var request = BuildRequest(System.Net.Http.HttpMethod.Delete, path, parameters, requestHeaders);
            var response = await SendAsync(request, skipAuth);
            return await ReadResponseAsync<T>(response);
        }

        public async Task<T?> PatchAsync<T>(
            string path,
            object? body = null,
            Dictionary<string, object>? parameters = null,
            Dictionary<string, string>? requestHeaders = null,
            string? contentType = null,
            bool skipAuth = false)
        {
            using var content = CreateContent(body, contentType);
            using var request = BuildRequest(System.Net.Http.HttpMethod.Patch, path, parameters, requestHeaders, content);
            var response = await SendAsync(request, skipAuth);
            return await ReadResponseAsync<T>(response);
        }
    }
}
