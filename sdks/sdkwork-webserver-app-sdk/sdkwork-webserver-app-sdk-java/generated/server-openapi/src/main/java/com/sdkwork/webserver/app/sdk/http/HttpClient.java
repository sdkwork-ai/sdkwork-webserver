package com.sdkwork.webserver.app.sdk.http;

import com.sdkwork.common.core.Types;
import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;
import okhttp3.*;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;

public class HttpClient {

    private final OkHttpClient client;
    private final ObjectMapper mapper;
    private final String baseUrl;
    private final Map<String, String> headers;

    public HttpClient(String baseUrl) {
        this(baseUrl, null, null);
    }

    public HttpClient(Types.SdkConfig config) {
        this(config.baseUrl(), config.timeout(), config.headers());
    }

    private HttpClient(String baseUrl, Integer timeout, Map<String, String> defaultHeaders) {
        this.baseUrl = baseUrl.endsWith("/") ? baseUrl.substring(0, baseUrl.length() - 1) : baseUrl;
        this.mapper = new ObjectMapper();
        this.headers = new HashMap<>();
        if (defaultHeaders != null) {
            this.headers.putAll(defaultHeaders);
        }

        long timeoutSeconds = timeout != null && timeout > 0 ? Math.max(1, timeout / 1000L) : 30;
        this.client = new OkHttpClient.Builder()
            .connectTimeout(timeoutSeconds, TimeUnit.SECONDS)
            .readTimeout(timeoutSeconds, TimeUnit.SECONDS)
            .writeTimeout(timeoutSeconds, TimeUnit.SECONDS)
            .build();
    }
    public void setAuthToken(String token) {
        headers.put("Authorization", "Bearer " + token);
    }
    public void setAccessToken(String token) {
        headers.put("Access-Token", token);
    }

    public void setHeader(String key, String value) {
        headers.put(key, value);
    }

    private HttpUrl buildUrl(String path, Map<String, Object> params) {
        HttpUrl.Builder urlBuilder = HttpUrl.parse(baseUrl + path).newBuilder();
        if (params != null) {
            for (Map.Entry<String, Object> entry : params.entrySet()) {
                urlBuilder.addQueryParameter(entry.getKey(), String.valueOf(entry.getValue()));
            }
        }
        return urlBuilder.build();
    }

    private Request.Builder applyHeaders(Request.Builder builder, Map<String, String> requestHeaders) {
        return applyHeaders(builder, requestHeaders, false, false);
    }

    private Request.Builder applyHeaders(Request.Builder builder, Map<String, String> requestHeaders, boolean skipAuth) {
        return applyHeaders(builder, requestHeaders, skipAuth, false);
    }

    private Request.Builder applyHeaders(
        Request.Builder builder,
        Map<String, String> requestHeaders,
        boolean skipAuth,
        boolean accessTokenOnly
    ) {
        Map<String, String> mergedHeaders = skipAuth || accessTokenOnly ? new HashMap<>() : new HashMap<>(headers);
        if (requestHeaders != null) {
            for (Map.Entry<String, String> entry : requestHeaders.entrySet()) {
                if (entry.getKey() != null && entry.getValue() != null
                    && ((!skipAuth && !accessTokenOnly) || !isCredentialHeader(entry.getKey()))) {
                    mergedHeaders.put(entry.getKey(), entry.getValue());
                }
            }
        }
        if (accessTokenOnly) {
            String accessToken = headers.entrySet().stream()
                .filter(entry -> "Access-Token".equalsIgnoreCase(entry.getKey()))
                .map(Map.Entry::getValue)
                .filter(value -> value != null && !value.trim().isEmpty())
                .map(String::trim)
                .findFirst()
                .orElseThrow(() -> new IllegalStateException(
                    "access-token-only request requires Access-Token before request dispatch"
                ));
            mergedHeaders.put("Access-Token", accessToken);
        }
        if (mergedHeaders.isEmpty()) {
            return builder;
        }
        return builder.headers(Headers.of(mergedHeaders));
    }

    private boolean isCredentialHeader(String key) {
        String normalized = key.toLowerCase(java.util.Locale.ROOT);
        return normalized.equals("authorization")
            || normalized.equals("access-token")
            || normalized.equals("x-api-key")
            || normalized.equals("x-tenant-id")
            || normalized.equals("x-organization-id")
            || normalized.equals("x-platform")
            || normalized.equals("x-user-id")
            || normalized.equals("x-sdkwork-tenant-id")
            || normalized.equals("x-sdkwork-organization-id")
            || normalized.equals("x-sdkwork-user-id");
    }

    private Object parseResponse(Response response) throws Exception {
        if (!response.isSuccessful()) {
            String body = response.body() != null ? response.body().string() : "";
            throw new RuntimeException("HTTP " + response.code() + ": " + body);
        }

        if (response.body() == null) {
            return null;
        }

        String bodyText = response.body().string();
        if (bodyText == null || bodyText.isBlank()) {
            return null;
        }

        return mapper.readValue(bodyText, Object.class);
    }

    private byte[] parseBinaryResponse(Response response) throws Exception {
        if (!response.isSuccessful()) {
            String body = response.body() != null ? response.body().string() : "";
            throw new RuntimeException("HTTP " + response.code() + ": " + body);
        }
        return response.body() == null ? new byte[0] : response.body().bytes();
    }

    public <T> T convertValue(Object value, TypeReference<T> typeReference) {
        if (value == null) {
            return null;
        }
        return mapper.convertValue(value, typeReference);
    }

    private RequestBody createJsonBody(Object body) throws Exception {
        Object payload = body == null ? new HashMap<String, Object>() : body;
        return RequestBody.create(
            mapper.writeValueAsString(payload),
            MediaType.parse("application/json")
        );
    }

    private RequestBody createMultipartBody(Object body) {
        if (body instanceof RequestBody) {
            RequestBody requestBody = (RequestBody) body;
            return requestBody;
        }

        MultipartBody.Builder builder = new MultipartBody.Builder().setType(MultipartBody.FORM);
        if (body instanceof Map<?, ?>) {
            Map<?, ?> mapBody = (Map<?, ?>) body;
            for (Map.Entry<?, ?> entry : mapBody.entrySet()) {
                if (entry.getKey() == null) {
                    continue;
                }
                String key = String.valueOf(entry.getKey());
                Object value = entry.getValue();
                if (value == null) {
                    builder.addFormDataPart(key, "");
                    continue;
                }
                if (value instanceof byte[]) {
                    byte[] bytes = (byte[]) value;
                    builder.addFormDataPart(
                        key,
                        key,
                        RequestBody.create(bytes, MediaType.parse("application/octet-stream"))
                    );
                    continue;
                }
                if (value instanceof Iterable<?>) {
                    Iterable<?> iterable = (Iterable<?>) value;
                    for (Object item : iterable) {
                        builder.addFormDataPart(key, item == null ? "" : String.valueOf(item));
                    }
                    continue;
                }
                if (value instanceof Collection<?>) {
                    Collection<?> collection = (Collection<?>) value;
                    for (Object item : collection) {
                        builder.addFormDataPart(key, item == null ? "" : String.valueOf(item));
                    }
                    continue;
                }
                builder.addFormDataPart(key, String.valueOf(value));
            }
        } else if (body != null) {
            builder.addFormDataPart("value", String.valueOf(body));
        }
        return builder.build();
    }

    private RequestBody createFormBody(Object body) {
        if (body instanceof RequestBody) {
            RequestBody requestBody = (RequestBody) body;
            return requestBody;
        }
        FormBody.Builder builder = new FormBody.Builder(StandardCharsets.UTF_8);
        if (body instanceof Map<?, ?>) {
            Map<?, ?> mapBody = (Map<?, ?>) body;
            for (Map.Entry<?, ?> entry : mapBody.entrySet()) {
                if (entry.getKey() == null) {
                    continue;
                }
                String key = String.valueOf(entry.getKey());
                Object value = entry.getValue();
                if (value == null) {
                    builder.add(key, "");
                    continue;
                }
                if (value instanceof Iterable<?>) {
                    Iterable<?> iterable = (Iterable<?>) value;
                    for (Object item : iterable) {
                        builder.add(key, item == null ? "" : String.valueOf(item));
                    }
                    continue;
                }
                if (value instanceof Collection<?>) {
                    Collection<?> collection = (Collection<?>) value;
                    for (Object item : collection) {
                        builder.add(key, item == null ? "" : String.valueOf(item));
                    }
                    continue;
                }
                builder.add(key, String.valueOf(value));
            }
        } else if (body != null) {
            builder.add("value", String.valueOf(body));
        }
        return builder.build();
    }

    private RequestBody createRequestBody(Object body, String contentType) throws Exception {
        String normalized = contentType == null || contentType.isBlank()
            ? "application/json"
            : contentType.toLowerCase();

        if (normalized.startsWith("multipart/form-data")) {
            return createMultipartBody(body);
        }
        if (normalized.startsWith("application/x-www-form-urlencoded")) {
            return createFormBody(body);
        }
        if (body instanceof RequestBody) {
            RequestBody requestBody = (RequestBody) body;
            return requestBody;
        }
        return createJsonBody(body);
    }

    private Object execute(Request request) throws Exception {
        try (Response response = client.newCall(request).execute()) {
            return parseResponse(response);
        }
    }

    public Object request(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType
    ) throws Exception {
        return request(method, path, body, params, requestHeaders, contentType, false, false);
    }

    private byte[] executeBytes(Request request) throws Exception {
        try (Response response = client.newCall(request).execute()) {
            return parseBinaryResponse(response);
        }
    }

    public Object request(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType,
        boolean skipAuth
    ) throws Exception {
        return request(method, path, body, params, requestHeaders, contentType, skipAuth, false);
    }

    public Object request(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType,
        boolean skipAuth,
        boolean accessTokenOnly
    ) throws Exception {
        RequestBody requestBody = body == null ? null : createRequestBody(body, contentType);
        Request request = applyHeaders(new Request.Builder(), requestHeaders, skipAuth, accessTokenOnly)
            .url(buildUrl(path, params))
            .method(method, requestBody)
            .build();
        return execute(request);
    }

    public byte[] requestBytes(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType,
        boolean skipAuth
    ) throws Exception {
        return requestBytes(method, path, body, params, requestHeaders, contentType, skipAuth, false);
    }

    public byte[] requestBytes(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType,
        boolean skipAuth,
        boolean accessTokenOnly
    ) throws Exception {
        RequestBody requestBody = body == null ? null : createRequestBody(body, contentType);
        Request request = applyHeaders(new Request.Builder(), requestHeaders, skipAuth, accessTokenOnly)
            .url(buildUrl(path, params))
            .method(method, requestBody)
            .build();
        return executeBytes(request);
    }

    public <T> Iterable<T> stream(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType,
        TypeReference<T> typeReference
    ) throws Exception {
        return stream(method, path, body, params, requestHeaders, contentType, typeReference, false, false);
    }

    public <T> Iterable<T> stream(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType,
        TypeReference<T> typeReference,
        boolean skipAuth
    ) throws Exception {
        return stream(method, path, body, params, requestHeaders, contentType, typeReference, skipAuth, false);
    }

    public <T> Iterable<T> stream(
        String method,
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType,
        TypeReference<T> typeReference,
        boolean skipAuth,
        boolean accessTokenOnly
    ) throws Exception {
        RequestBody requestBody = body == null ? null : createRequestBody(body, contentType);
        Request request = applyHeaders(new Request.Builder(), requestHeaders, skipAuth, accessTokenOnly)
            .url(buildUrl(path, params))
            .addHeader("Accept", "text/event-stream")
            .method(method, requestBody)
            .build();
        Response response = client.newCall(request).execute();
        if (!response.isSuccessful()) {
            String responseBody = response.body() != null ? response.body().string() : "";
            response.close();
            throw new RuntimeException("HTTP " + response.code() + ": " + responseBody);
        }
        List<T> events = new ArrayList<>();
        try (Response closeableResponse = response;
             BufferedReader reader = new BufferedReader(new InputStreamReader(closeableResponse.body().byteStream(), StandardCharsets.UTF_8))) {
            String line;
            while ((line = reader.readLine()) != null) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith(":") || !line.startsWith("data:")) {
                    continue;
                }
                String data = line.substring(5).trim();
                if ("[DONE]".equals(data)) {
                    break;
                }
                events.add(mapper.readValue(data, typeReference));
            }
        }
        return events;
    }

    public Object get(String path) throws Exception {
        return get(path, null, null);
    }

    public Object get(String path, Map<String, Object> params) throws Exception {
        return get(path, params, null);
    }

    public Object get(String path, Map<String, Object> params, Map<String, String> requestHeaders) throws Exception {
        Request request = applyHeaders(new Request.Builder(), requestHeaders)
            .url(buildUrl(path, params))
            .get()
            .build();
        return execute(request);
    }

    public Object post(String path, Object body) throws Exception {
        return post(path, body, null, null, "application/json");
    }

    public Object post(String path, Object body, Map<String, Object> params) throws Exception {
        return post(path, body, params, null, "application/json");
    }

    public Object post(String path, Object body, Map<String, Object> params, Map<String, String> requestHeaders) throws Exception {
        return post(path, body, params, requestHeaders, "application/json");
    }

    public Object post(
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType
    ) throws Exception {
        RequestBody requestBody = createRequestBody(body, contentType);
        Request request = applyHeaders(new Request.Builder(), requestHeaders)
            .url(buildUrl(path, params))
            .post(requestBody)
            .build();
        return execute(request);
    }

    public Object put(String path, Object body) throws Exception {
        return put(path, body, null, null, "application/json");
    }

    public Object put(String path, Object body, Map<String, Object> params) throws Exception {
        return put(path, body, params, null, "application/json");
    }

    public Object put(String path, Object body, Map<String, Object> params, Map<String, String> requestHeaders) throws Exception {
        return put(path, body, params, requestHeaders, "application/json");
    }

    public Object put(
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType
    ) throws Exception {
        RequestBody requestBody = createRequestBody(body, contentType);
        Request request = applyHeaders(new Request.Builder(), requestHeaders)
            .url(buildUrl(path, params))
            .put(requestBody)
            .build();
        return execute(request);
    }

    public Object delete(String path) throws Exception {
        return delete(path, null, null);
    }

    public Object delete(String path, Map<String, Object> params) throws Exception {
        return delete(path, params, null);
    }

    public Object delete(String path, Map<String, Object> params, Map<String, String> requestHeaders) throws Exception {
        Request request = applyHeaders(new Request.Builder(), requestHeaders)
            .url(buildUrl(path, params))
            .delete()
            .build();
        return execute(request);
    }

    public Object patch(String path, Object body) throws Exception {
        return patch(path, body, null, null, "application/json");
    }

    public Object patch(String path, Object body, Map<String, Object> params) throws Exception {
        return patch(path, body, params, null, "application/json");
    }

    public Object patch(String path, Object body, Map<String, Object> params, Map<String, String> requestHeaders) throws Exception {
        return patch(path, body, params, requestHeaders, "application/json");
    }

    public Object patch(
        String path,
        Object body,
        Map<String, Object> params,
        Map<String, String> requestHeaders,
        String contentType
    ) throws Exception {
        RequestBody requestBody = createRequestBody(body, contentType);
        Request request = applyHeaders(new Request.Builder(), requestHeaders)
            .url(buildUrl(path, params))
            .patch(requestBody)
            .build();
        return execute(request);
    }
}
