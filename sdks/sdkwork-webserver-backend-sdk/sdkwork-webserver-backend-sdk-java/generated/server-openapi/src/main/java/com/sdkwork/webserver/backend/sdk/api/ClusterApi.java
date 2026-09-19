package com.sdkwork.webserver.backend.sdk.api;

import com.fasterxml.jackson.core.type.TypeReference;
import com.sdkwork.webserver.backend.sdk.http.HttpClient;
import com.sdkwork.webserver.backend.sdk.model.*;
import java.util.List;
import java.util.Map;

public class ClusterApi {
    private final HttpClient client;

    public ClusterApi(HttpClient client) {
        this.client = client;
    }

    /** List Web Server clusters */
    public ClustersListResponse clustersList(Integer page, Integer pageSize) throws Exception {
        String query = buildQueryString(List.of(
            new QueryParameterSpec("page", page, "form", true, false, null),
            new QueryParameterSpec("page_size", pageSize, "form", true, false, null)
        ));
        Object raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters"), query));
        return client.convertValue(raw, new TypeReference<ClustersListResponse>() {});
    }

    /** Create a Web Server cluster */
    public ClustersCreateResponse201 clustersCreate(CreateClusterRequest body, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        Object raw = client.post(ApiPaths.backendPath("/clusters"), body, null, requestHeaders, "application/json");
        return client.convertValue(raw, new TypeReference<ClustersCreateResponse201>() {});
    }

    /** Retrieve a Web Server cluster */
    public ClustersRetrieveResponse clustersRetrieve(String clusterId) throws Exception {
        Object raw = client.get(ApiPaths.backendPath("/clusters/" + serializePathParameter(clusterId, new PathParameterSpec("clusterId", "simple", false)) + ""));
        return client.convertValue(raw, new TypeReference<ClustersRetrieveResponse>() {});
    }

    /** Update a Web Server cluster */
    public ClustersUpdateResponse clustersUpdate(String clusterId, UpdateClusterRequest body, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        Object raw = client.patch(ApiPaths.backendPath("/clusters/" + serializePathParameter(clusterId, new PathParameterSpec("clusterId", "simple", false)) + ""), body, null, requestHeaders, "application/json");
        return client.convertValue(raw, new TypeReference<ClustersUpdateResponse>() {});
    }

    /** Delete an empty Web Server cluster */
    public Void clustersDelete(String clusterId, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        client.delete(ApiPaths.backendPath("/clusters/" + serializePathParameter(clusterId, new PathParameterSpec("clusterId", "simple", false)) + ""), null, requestHeaders);
        return null;
    }

    /** List cluster hosts with system and network identity */
    public ClustersHostsListResponse clustersHostsList(Integer pageSize, String cursor, String clusterId, Integer status) throws Exception {
        String query = buildQueryString(List.of(
            new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            new QueryParameterSpec("cursor", cursor, "form", true, false, null),
            new QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
            new QueryParameterSpec("status", status, "form", true, false, null)
        ));
        Object raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/hosts"), query));
        return client.convertValue(raw, new TypeReference<ClustersHostsListResponse>() {});
    }

    /** Retrieve a cluster host */
    public ClustersHostsRetrieveResponse clustersHostsRetrieve(String hostId) throws Exception {
        Object raw = client.get(ApiPaths.backendPath("/clusters/hosts/" + serializePathParameter(hostId, new PathParameterSpec("hostId", "simple", false)) + ""));
        return client.convertValue(raw, new TypeReference<ClustersHostsRetrieveResponse>() {});
    }

    /** Rename a host or reassign it to another cluster */
    public ClustersHostsUpdateResponse clustersHostsUpdate(String hostId, UpdateClusterHostRequest body, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        Object raw = client.patch(ApiPaths.backendPath("/clusters/hosts/" + serializePathParameter(hostId, new PathParameterSpec("hostId", "simple", false)) + ""), body, null, requestHeaders, "application/json");
        return client.convertValue(raw, new TypeReference<ClustersHostsUpdateResponse>() {});
    }

    /** Remove an instance-free host from the cluster inventory */
    public Void clustersHostsDelete(String hostId, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        client.delete(ApiPaths.backendPath("/clusters/hosts/" + serializePathParameter(hostId, new PathParameterSpec("hostId", "simple", false)) + ""), null, requestHeaders);
        return null;
    }

    /** List webserver process instances with liveness state */
    public ClustersInstancesListResponse clustersInstancesList(Integer pageSize, String cursor, String clusterId, String hostId, Integer status, String healthState) throws Exception {
        String query = buildQueryString(List.of(
            new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            new QueryParameterSpec("cursor", cursor, "form", true, false, null),
            new QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
            new QueryParameterSpec("host_id", hostId, "form", true, false, null),
            new QueryParameterSpec("status", status, "form", true, false, null),
            new QueryParameterSpec("health_state", healthState, "form", true, false, null)
        ));
        Object raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances"), query));
        return client.convertValue(raw, new TypeReference<ClustersInstancesListResponse>() {});
    }

    /** Retrieve a webserver process instance */
    public ClustersInstancesRetrieveResponse clustersInstancesRetrieve(String instanceId) throws Exception {
        Object raw = client.get(ApiPaths.backendPath("/clusters/instances/" + serializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false)) + ""));
        return client.convertValue(raw, new TypeReference<ClustersInstancesRetrieveResponse>() {});
    }

    /** Update an instance display name, status, or advertised endpoint */
    public ClustersInstancesUpdateResponse clustersInstancesUpdate(String instanceId, UpdateClusterInstanceRequest body, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        Object raw = client.patch(ApiPaths.backendPath("/clusters/instances/" + serializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false)) + ""), body, null, requestHeaders, "application/json");
        return client.convertValue(raw, new TypeReference<ClustersInstancesUpdateResponse>() {});
    }

    /** Unregister a webserver process instance */
    public Void clustersInstancesDelete(String instanceId, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        client.delete(ApiPaths.backendPath("/clusters/instances/" + serializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false)) + ""), null, requestHeaders);
        return null;
    }

    /** List cluster lifecycle events */
    public ClustersEventsListResponse clustersEventsList(Integer pageSize, String cursor, String clusterId, String severity) throws Exception {
        String query = buildQueryString(List.of(
            new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            new QueryParameterSpec("cursor", cursor, "form", true, false, null),
            new QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
            new QueryParameterSpec("severity", severity, "form", true, false, null)
        ));
        Object raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/events"), query));
        return client.convertValue(raw, new TypeReference<ClustersEventsListResponse>() {});
    }

    /** Retrieve the cluster health overview for status polling */
    public ClustersOverviewRetrieveResponse clustersOverviewRetrieve() throws Exception {
        Object raw = client.get(ApiPaths.backendPath("/clusters/overview"));
        return client.convertValue(raw, new TypeReference<ClustersOverviewRetrieveResponse>() {});
    }

    /** List one instance's stored heartbeat samples */
    public ClustersInstancesHeartbeatsListResponse clustersInstancesHeartbeatsList(String instanceId, Integer pageSize, String cursor) throws Exception {
        String query = buildQueryString(List.of(
            new QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            new QueryParameterSpec("cursor", cursor, "form", true, false, null)
        ));
        Object raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances/" + serializePathParameter(instanceId, new PathParameterSpec("instanceId", "simple", false)) + "/heartbeats"), query));
        return client.convertValue(raw, new TypeReference<ClustersInstancesHeartbeatsListResponse>() {});
    }

    /** Enqueue a peer message to one instance or broadcast to online members */
    public ClustersMessagesCreateResponse201 clustersMessagesCreate(EnqueueClusterPeerMessagesRequest body, String idempotencyKey) throws Exception {
        Map<String, String> requestHeaders = buildRequestHeaders(
                Map.of("Idempotency-Key", new HeaderParameterSpec(idempotencyKey, "simple", false, null)),
                Map.of()
        );
        Object raw = client.post(ApiPaths.backendPath("/clusters/messages"), body, null, requestHeaders, "application/json");
        return client.convertValue(raw, new TypeReference<ClustersMessagesCreateResponse201>() {});
    }

    private record PathParameterSpec(String name, String style, boolean explode) {}

    private static String serializePathParameter(Object value, PathParameterSpec spec) {
        if (value == null) {
            return "";
        }
        String style = spec.style() == null || spec.style().isBlank() ? "simple" : spec.style();
        if (value instanceof Iterable<?> iterable) {
            return serializePathArray(spec.name(), iterable, style, spec.explode());
        }
        if (value instanceof Map<?, ?> map) {
            return serializePathObject(spec.name(), map, style, spec.explode());
        }
        return pathPrimitivePrefix(spec.name(), style) + pathEncode(String.valueOf(value));
    }

    private static String serializePathArray(String name, Iterable<?> values, String style, boolean explode) {
        List<String> serialized = new java.util.ArrayList<>();
        for (Object item : values) {
            if (item != null) {
                serialized.add(pathEncode(String.valueOf(item)));
            }
        }
        if (serialized.isEmpty()) {
            return pathPrefix(name, style);
        }
        if ("matrix".equals(style)) {
            if (explode) {
                List<String> parts = new java.util.ArrayList<>();
                for (String item : serialized) {
                    parts.add(";" + name + "=" + item);
                }
                return String.join("", parts);
            }
            return ";" + name + "=" + String.join(",", serialized);
        }
        String separator = explode ? "." : ",";
        return pathPrefix(name, style) + String.join(separator, serialized);
    }

    private static String serializePathObject(String name, Map<?, ?> values, String style, boolean explode) {
        List<String> entries = new java.util.ArrayList<>();
        List<String> exploded = new java.util.ArrayList<>();
        values.forEach((key, value) -> {
            if (value == null) {
                return;
            }
            String escapedKey = pathEncode(String.valueOf(key));
            String escapedValue = pathEncode(String.valueOf(value));
            if (explode) {
                if ("matrix".equals(style)) {
                    exploded.add(";" + escapedKey + "=" + escapedValue);
                } else {
                    exploded.add(escapedKey + "=" + escapedValue);
                }
            } else {
                entries.add(escapedKey);
                entries.add(escapedValue);
            }
        });
        if ("matrix".equals(style)) {
            if (explode) {
                return String.join("", exploded);
            }
            return ";" + name + "=" + String.join(",", entries);
        }
        if (explode) {
            String separator = "label".equals(style) ? "." : ",";
            return pathPrefix(name, style) + String.join(separator, exploded);
        }
        return pathPrefix(name, style) + String.join(",", entries);
    }

    private static String pathPrefix(String name, String style) {
        if ("label".equals(style)) {
            return ".";
        }
        if ("matrix".equals(style)) {
            return ";" + name;
        }
        return "";
    }

    private static String pathPrimitivePrefix(String name, String style) {
        if ("matrix".equals(style)) {
            return ";" + name + "=";
        }
        return pathPrefix(name, style);
    }

    private static String pathEncode(String value) {
        return java.net.URLEncoder.encode(value, java.nio.charset.StandardCharsets.UTF_8).replace("+", "%20");
    }

    private record QueryParameterSpec(String name, Object value, String style, boolean explode, boolean allowReserved, String contentType) {}

    private static String buildQueryString(List<QueryParameterSpec> parameters) throws Exception {
        List<String> pairs = new java.util.ArrayList<>();
        for (QueryParameterSpec parameter : parameters) {
            appendSerializedParameter(pairs, parameter);
        }
        return String.join("&", pairs);
    }

    private static void appendSerializedParameter(List<String> pairs, QueryParameterSpec parameter) throws Exception {
        if (parameter.value() == null) {
            return;
        }
        if (parameter.contentType() != null && !parameter.contentType().isBlank()) {
            String json = clientObjectMapper().writeValueAsString(parameter.value());
            pairs.add(urlEncode(parameter.name()) + "=" + encodeQueryValue(json, parameter.allowReserved()));
            return;
        }

        String style = parameter.style() == null || parameter.style().isBlank() ? "form" : parameter.style();
        Object value = parameter.value();
        if ("deepObject".equals(style) && value instanceof Map<?, ?> map) {
            appendDeepObjectParameter(pairs, parameter.name(), map, parameter.allowReserved());
        } else if (value instanceof Iterable<?> iterable) {
            appendArrayParameter(pairs, parameter.name(), iterable, style, parameter.explode(), parameter.allowReserved());
        } else if (value instanceof Map<?, ?> map) {
            appendObjectParameter(pairs, parameter.name(), map, style, parameter.explode(), parameter.allowReserved());
        } else {
            pairs.add(urlEncode(parameter.name()) + "=" + encodeQueryValue(String.valueOf(value), parameter.allowReserved()));
        }
    }

    private static void appendArrayParameter(List<String> pairs, String name, Iterable<?> values, String style, boolean explode, boolean allowReserved) {
        List<String> serialized = new java.util.ArrayList<>();
        for (Object item : values) {
            if (item != null) {
                serialized.add(String.valueOf(item));
            }
        }
        if (serialized.isEmpty()) {
            return;
        }
        if ("form".equals(style) && explode) {
            for (String item : serialized) {
                pairs.add(urlEncode(name) + "=" + encodeQueryValue(item, allowReserved));
            }
            return;
        }
        pairs.add(urlEncode(name) + "=" + encodeQueryValue(String.join(",", serialized), allowReserved));
    }

    private static void appendObjectParameter(List<String> pairs, String name, Map<?, ?> values, String style, boolean explode, boolean allowReserved) {
        List<String> serialized = new java.util.ArrayList<>();
        values.forEach((key, value) -> {
            if (value == null) {
                return;
            }
            if ("form".equals(style) && explode) {
                pairs.add(urlEncode(String.valueOf(key)) + "=" + encodeQueryValue(String.valueOf(value), allowReserved));
            } else {
                serialized.add(String.valueOf(key));
                serialized.add(String.valueOf(value));
            }
        });
        if (!serialized.isEmpty()) {
            pairs.add(urlEncode(name) + "=" + encodeQueryValue(String.join(",", serialized), allowReserved));
        }
    }

    private static void appendDeepObjectParameter(List<String> pairs, String name, Map<?, ?> values, boolean allowReserved) {
        values.forEach((key, value) -> {
            if (value != null) {
                pairs.add(urlEncode(name + "[" + key + "]") + "=" + encodeQueryValue(String.valueOf(value), allowReserved));
            }
        });
    }

    private static String encodeQueryValue(String value, boolean allowReserved) {
        String encoded = urlEncode(value);
        if (!allowReserved) {
            return encoded;
        }
        return encoded
            .replace("%3A", ":").replace("%2F", "/").replace("%3F", "?").replace("%23", "#")
            .replace("%5B", "[").replace("%5D", "]").replace("%40", "@").replace("%21", "!")
            .replace("%24", "$").replace("%26", "&").replace("%27", "'").replace("%28", "(")
            .replace("%29", ")").replace("%2A", "*").replace("%2B", "+").replace("%2C", ",")
            .replace("%3B", ";").replace("%3D", "=");
    }

    private static com.fasterxml.jackson.databind.ObjectMapper clientObjectMapper() {
        return new com.fasterxml.jackson.databind.ObjectMapper();
    }

    private record HeaderParameterSpec(Object value, String style, boolean explode, String contentType) {}

    private static Map<String, String> buildRequestHeaders(Map<String, HeaderParameterSpec> headers, Map<String, HeaderParameterSpec> cookies) throws Exception {
        Map<String, String> requestHeaders = new java.util.LinkedHashMap<>();
        for (Map.Entry<String, HeaderParameterSpec> entry : headers.entrySet()) {
            String serialized = serializeParameterValue(entry.getValue());
            if (serialized != null) {
                requestHeaders.put(entry.getKey(), serialized);
            }
        }

        String cookieHeader = buildCookieHeader(cookies);
        if (cookieHeader != null && !cookieHeader.isEmpty()) {
            requestHeaders.merge("Cookie", cookieHeader, (left, right) -> left + "; " + right);
        }

        return requestHeaders.isEmpty() ? null : requestHeaders;
    }

    private static String buildCookieHeader(Map<String, HeaderParameterSpec> cookies) throws Exception {
        java.util.List<String> pairs = new java.util.ArrayList<>();
        for (Map.Entry<String, HeaderParameterSpec> entry : cookies.entrySet()) {
            String serialized = serializeParameterValue(entry.getValue());
            if (serialized != null) {
                pairs.add(urlEncode(entry.getKey()) + "=" + urlEncode(serialized));
            }
        }
        return String.join("; ", pairs);
    }

    private static String serializeParameterValue(HeaderParameterSpec parameter) throws Exception {
        if (parameter == null || parameter.value() == null) {
            return null;
        }
        Object value = parameter.value();
        if (parameter.contentType() != null && !parameter.contentType().isBlank()) {
            return headerObjectMapper().writeValueAsString(value);
        }
        if (value instanceof Iterable<?> iterable) {
            java.util.List<String> values = new java.util.ArrayList<>();
            for (Object item : iterable) {
                if (item != null) {
                    values.add(String.valueOf(item));
                }
            }
            return String.join(",", values);
        }
        if (value instanceof Map<?, ?> map) {
            java.util.List<String> values = new java.util.ArrayList<>();
            map.forEach((key, item) -> {
                if (item == null) {
                    return;
                }
                if (parameter.explode()) {
                    values.add(String.valueOf(key) + "=" + String.valueOf(item));
                } else {
                    values.add(String.valueOf(key));
                    values.add(String.valueOf(item));
                }
            });
            return String.join(",", values);
        }
        return String.valueOf(value);
    }

    private static com.fasterxml.jackson.databind.ObjectMapper headerObjectMapper() {
        return new com.fasterxml.jackson.databind.ObjectMapper();
    }

    private static String urlEncode(String value) {
        return java.net.URLEncoder.encode(value, java.nio.charset.StandardCharsets.UTF_8);
    }
}
