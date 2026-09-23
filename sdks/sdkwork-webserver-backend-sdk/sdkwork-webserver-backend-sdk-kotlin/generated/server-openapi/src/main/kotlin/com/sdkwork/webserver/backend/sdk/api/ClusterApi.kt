package com.sdkwork.webserver.backend.sdk.api

import com.fasterxml.jackson.core.type.TypeReference
import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.module.kotlin.registerKotlinModule
import com.sdkwork.webserver.backend.sdk.*
import com.sdkwork.webserver.backend.sdk.http.HttpClient

class ClusterApi(private val client: HttpClient) {

    /** List Web Server clusters */
    suspend fun clustersList(page: Int? = null, pageSize: Int? = null): ClustersListResponse? {
        val query = buildQueryString(listOf(
            QueryParameterSpec("page", page, "form", true, false, null),
            QueryParameterSpec("page_size", pageSize, "form", true, false, null)
        ))
        val raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters"), query))
        return client.convertValue(raw, object : TypeReference<ClustersListResponse>() {})
    }

    /** Create a Web Server cluster */
    suspend fun clustersCreate(body: CreateClusterRequest, idempotencyKey: String): ClustersCreateResponse201? {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        val raw = client.post(ApiPaths.backendPath("/clusters"), body, null, requestHeaders, "application/json")
        return client.convertValue(raw, object : TypeReference<ClustersCreateResponse201>() {})
    }

    /** Retrieve a Web Server cluster */
    suspend fun clustersRetrieve(clusterId: String): ClustersRetrieveResponse? {
        val raw = client.get(ApiPaths.backendPath("/clusters/${serializePathParameter(clusterId, PathParameterSpec("clusterId", "simple", false))}"))
        return client.convertValue(raw, object : TypeReference<ClustersRetrieveResponse>() {})
    }

    /** Update a Web Server cluster */
    suspend fun clustersUpdate(clusterId: String, body: UpdateClusterRequest, idempotencyKey: String): ClustersUpdateResponse? {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        val raw = client.patch(ApiPaths.backendPath("/clusters/${serializePathParameter(clusterId, PathParameterSpec("clusterId", "simple", false))}"), body, null, requestHeaders, "application/json")
        return client.convertValue(raw, object : TypeReference<ClustersUpdateResponse>() {})
    }

    /** Delete an empty Web Server cluster */
    suspend fun clustersDelete(clusterId: String, idempotencyKey: String): Unit {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        client.delete(ApiPaths.backendPath("/clusters/${serializePathParameter(clusterId, PathParameterSpec("clusterId", "simple", false))}"), null, requestHeaders)
    }

    /** Publish a desired-state revision to every instance of the cluster */
    suspend fun clustersSync(clusterId: String, body: PublishClusterSyncRequest): ClustersSyncResponse? {
        val raw = client.post(ApiPaths.backendPath("/clusters/${serializePathParameter(clusterId, PathParameterSpec("clusterId", "simple", false))}/sync"), body, null, null, "application/json")
        return client.convertValue(raw, object : TypeReference<ClustersSyncResponse>() {})
    }

    /** List cluster hosts with system and network identity */
    suspend fun clustersHostsList(pageSize: Int? = null, cursor: String? = null, clusterId: String? = null, status: Int? = null): ClustersHostsListResponse? {
        val query = buildQueryString(listOf(
            QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            QueryParameterSpec("cursor", cursor, "form", true, false, null),
            QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
            QueryParameterSpec("status", status, "form", true, false, null)
        ))
        val raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/hosts"), query))
        return client.convertValue(raw, object : TypeReference<ClustersHostsListResponse>() {})
    }

    /** Retrieve a cluster host */
    suspend fun clustersHostsRetrieve(hostId: String): ClustersHostsRetrieveResponse? {
        val raw = client.get(ApiPaths.backendPath("/clusters/hosts/${serializePathParameter(hostId, PathParameterSpec("hostId", "simple", false))}"))
        return client.convertValue(raw, object : TypeReference<ClustersHostsRetrieveResponse>() {})
    }

    /** Rename a host or reassign it to another cluster */
    suspend fun clustersHostsUpdate(hostId: String, body: UpdateClusterHostRequest, idempotencyKey: String): ClustersHostsUpdateResponse? {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        val raw = client.patch(ApiPaths.backendPath("/clusters/hosts/${serializePathParameter(hostId, PathParameterSpec("hostId", "simple", false))}"), body, null, requestHeaders, "application/json")
        return client.convertValue(raw, object : TypeReference<ClustersHostsUpdateResponse>() {})
    }

    /** Remove an instance-free host from the cluster inventory */
    suspend fun clustersHostsDelete(hostId: String, idempotencyKey: String): Unit {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        client.delete(ApiPaths.backendPath("/clusters/hosts/${serializePathParameter(hostId, PathParameterSpec("hostId", "simple", false))}"), null, requestHeaders)
    }

    /** List webserver process instances with liveness state */
    suspend fun clustersInstancesList(pageSize: Int? = null, cursor: String? = null, clusterId: String? = null, hostId: String? = null, status: Int? = null, healthState: String? = null): ClustersInstancesListResponse? {
        val query = buildQueryString(listOf(
            QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            QueryParameterSpec("cursor", cursor, "form", true, false, null),
            QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
            QueryParameterSpec("host_id", hostId, "form", true, false, null),
            QueryParameterSpec("status", status, "form", true, false, null),
            QueryParameterSpec("health_state", healthState, "form", true, false, null)
        ))
        val raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances"), query))
        return client.convertValue(raw, object : TypeReference<ClustersInstancesListResponse>() {})
    }

    /** Retrieve a webserver process instance */
    suspend fun clustersInstancesRetrieve(instanceId: String): ClustersInstancesRetrieveResponse? {
        val raw = client.get(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}"))
        return client.convertValue(raw, object : TypeReference<ClustersInstancesRetrieveResponse>() {})
    }

    /** Update an instance display name, status, or advertised endpoint */
    suspend fun clustersInstancesUpdate(instanceId: String, body: UpdateClusterInstanceRequest, idempotencyKey: String): ClustersInstancesUpdateResponse? {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        val raw = client.patch(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}"), body, null, requestHeaders, "application/json")
        return client.convertValue(raw, object : TypeReference<ClustersInstancesUpdateResponse>() {})
    }

    /** Unregister a webserver process instance */
    suspend fun clustersInstancesDelete(instanceId: String, idempotencyKey: String): Unit {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        client.delete(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}"), null, requestHeaders)
    }

    /** List cluster lifecycle events */
    suspend fun clustersEventsList(pageSize: Int? = null, cursor: String? = null, clusterId: String? = null, severity: String? = null): ClustersEventsListResponse? {
        val query = buildQueryString(listOf(
            QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            QueryParameterSpec("cursor", cursor, "form", true, false, null),
            QueryParameterSpec("cluster_id", clusterId, "form", true, false, null),
            QueryParameterSpec("severity", severity, "form", true, false, null)
        ))
        val raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/events"), query))
        return client.convertValue(raw, object : TypeReference<ClustersEventsListResponse>() {})
    }

    /** Retrieve the cluster health overview for status polling */
    suspend fun clustersOverviewRetrieve(): ClustersOverviewRetrieveResponse? {
        val raw = client.get(ApiPaths.backendPath("/clusters/overview"))
        return client.convertValue(raw, object : TypeReference<ClustersOverviewRetrieveResponse>() {})
    }

    /** List one instance's stored heartbeat samples */
    suspend fun clustersInstancesHeartbeatsList(instanceId: String, pageSize: Int? = null, cursor: String? = null): ClustersInstancesHeartbeatsListResponse? {
        val query = buildQueryString(listOf(
            QueryParameterSpec("page_size", pageSize, "form", true, false, null),
            QueryParameterSpec("cursor", cursor, "form", true, false, null)
        ))
        val raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}/heartbeats"), query))
        return client.convertValue(raw, object : TypeReference<ClustersInstancesHeartbeatsListResponse>() {})
    }

    /** List one instance's heartbeat metric samples for trend charts */
    suspend fun clustersInstancesMetricsList(instanceId: String, limit: Int? = null): ClustersInstancesMetricsListResponse? {
        val query = buildQueryString(listOf(
            QueryParameterSpec("limit", limit, "form", true, false, null)
        ))
        val raw = client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}/metrics/history"), query))
        return client.convertValue(raw, object : TypeReference<ClustersInstancesMetricsListResponse>() {})
    }

    /** Probe one instance's connectivity and record the outcome */
    suspend fun clustersInstancesProbe(instanceId: String, body: ProbeClusterInstanceRequest? = null): ClustersInstancesProbeResponse? {
        val raw = client.post(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}/probe"), body, null, null, "application/json")
        return client.convertValue(raw, object : TypeReference<ClustersInstancesProbeResponse>() {})
    }

    /** Gracefully drain one instance out of routing */
    suspend fun clustersInstancesDrain(instanceId: String): ClustersInstancesDrainResponse? {
        val raw = client.post(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}/drain"), null)
        return client.convertValue(raw, object : TypeReference<ClustersInstancesDrainResponse>() {})
    }

    /** Clear the drain flag and restore routing participation */
    suspend fun clustersInstancesUndrain(instanceId: String): ClustersInstancesUndrainResponse? {
        val raw = client.post(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}/undrain"), null)
        return client.convertValue(raw, object : TypeReference<ClustersInstancesUndrainResponse>() {})
    }

    /** Cordon one instance out of routing without draining it */
    suspend fun clustersInstancesCordon(instanceId: String): ClustersInstancesCordonResponse? {
        val raw = client.post(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}/cordon"), null)
        return client.convertValue(raw, object : TypeReference<ClustersInstancesCordonResponse>() {})
    }

    /** Uncordon one instance back into routing */
    suspend fun clustersInstancesUncordon(instanceId: String): ClustersInstancesUncordonResponse? {
        val raw = client.post(ApiPaths.backendPath("/clusters/instances/${serializePathParameter(instanceId, PathParameterSpec("instanceId", "simple", false))}/uncordon"), null)
        return client.convertValue(raw, object : TypeReference<ClustersInstancesUncordonResponse>() {})
    }

    /** Enqueue a peer message to one instance or broadcast to online members */
    suspend fun clustersMessagesCreate(body: EnqueueClusterPeerMessagesRequest, idempotencyKey: String): ClustersMessagesCreateResponse201? {
        val requestHeaders = buildRequestHeaders(
            mapOf(
                "Idempotency-Key" to HeaderParameterSpec(idempotencyKey, "simple", false, null),
            ),
            emptyMap()
        )
        val raw = client.post(ApiPaths.backendPath("/clusters/messages"), body, null, requestHeaders, "application/json")
        return client.convertValue(raw, object : TypeReference<ClustersMessagesCreateResponse201>() {})
    }

    private data class PathParameterSpec(val name: String, val style: String, val explode: Boolean)

    private fun serializePathParameter(value: Any?, spec: PathParameterSpec): String {
        if (value == null) return ""
        val style = spec.style.ifBlank { "simple" }
        return when (value) {
            is Iterable<*> -> serializePathArray(spec.name, value, style, spec.explode)
            is Map<*, *> -> serializePathObject(spec.name, value, style, spec.explode)
            else -> pathPrimitivePrefix(spec.name, style) + pathEncode(value.toString())
        }
    }

    private fun serializePathArray(name: String, values: Iterable<*>, style: String, explode: Boolean): String {
        val serialized = values.mapNotNull { it?.toString()?.let(::pathEncode) }
        if (serialized.isEmpty()) return pathPrefix(name, style)
        if (style == "matrix") {
            if (explode) {
                return serialized.joinToString("") { ";$name=$it" }
            }
            return ";$name=" + serialized.joinToString(",")
        }
        val separator = if (explode) "." else ","
        return pathPrefix(name, style) + serialized.joinToString(separator)
    }

    private fun serializePathObject(name: String, values: Map<*, *>, style: String, explode: Boolean): String {
        val entries = mutableListOf<String>()
        val exploded = mutableListOf<String>()
        values.forEach { (key, value) ->
            if (value == null) return@forEach
            val escapedKey = pathEncode(key.toString())
            val escapedValue = pathEncode(value.toString())
            if (explode) {
                if (style == "matrix") {
                    exploded += ";$escapedKey=$escapedValue"
                } else {
                    exploded += "$escapedKey=$escapedValue"
                }
            } else {
                entries += escapedKey
                entries += escapedValue
            }
        }
        if (style == "matrix") {
            if (explode) return exploded.joinToString("")
            return ";$name=" + entries.joinToString(",")
        }
        if (explode) {
            val separator = if (style == "label") "." else ","
            return pathPrefix(name, style) + exploded.joinToString(separator)
        }
        return pathPrefix(name, style) + entries.joinToString(",")
    }

    private fun pathPrefix(name: String, style: String): String {
        return when (style) {
            "label" -> "."
            "matrix" -> ";$name"
            else -> ""
        }
    }

    private fun pathPrimitivePrefix(name: String, style: String): String {
        return if (style == "matrix") ";$name=" else pathPrefix(name, style)
    }

    private fun pathEncode(value: String): String {
        return java.net.URLEncoder.encode(value, java.nio.charset.StandardCharsets.UTF_8).replace("+", "%20")
    }

    private data class QueryParameterSpec(
        val name: String,
        val value: Any?,
        val style: String,
        val explode: Boolean,
        val allowReserved: Boolean,
        val contentType: String?,
    )

    private val queryObjectMapper = ObjectMapper().registerKotlinModule()

    private fun buildQueryString(parameters: List<QueryParameterSpec>): String {
        val pairs = mutableListOf<String>()
        parameters.forEach { appendSerializedParameter(pairs, it) }
        return pairs.joinToString("&")
    }

    private fun appendSerializedParameter(pairs: MutableList<String>, parameter: QueryParameterSpec) {
        val value = parameter.value ?: return
        if (!parameter.contentType.isNullOrBlank()) {
            val json = queryObjectMapper.writeValueAsString(value)
            pairs += urlEncode(parameter.name) + "=" + encodeQueryValue(json, parameter.allowReserved)
            return
        }

        val style = parameter.style.ifBlank { "form" }
        when (value) {
            is Iterable<*> -> appendArrayParameter(pairs, parameter.name, value, style, parameter.explode, parameter.allowReserved)
            is Map<*, *> -> if (style == "deepObject") {
                appendDeepObjectParameter(pairs, parameter.name, value, parameter.allowReserved)
            } else {
                appendObjectParameter(pairs, parameter.name, value, style, parameter.explode, parameter.allowReserved)
            }
            else -> pairs += urlEncode(parameter.name) + "=" + encodeQueryValue(value.toString(), parameter.allowReserved)
        }
    }

    private fun appendArrayParameter(
        pairs: MutableList<String>,
        name: String,
        values: Iterable<*>,
        style: String,
        explode: Boolean,
        allowReserved: Boolean,
    ) {
        val serialized = values.mapNotNull { it?.toString() }
        if (serialized.isEmpty()) return
        if (style == "form" && explode) {
            serialized.forEach { pairs += urlEncode(name) + "=" + encodeQueryValue(it, allowReserved) }
            return
        }
        pairs += urlEncode(name) + "=" + encodeQueryValue(serialized.joinToString(","), allowReserved)
    }

    private fun appendObjectParameter(
        pairs: MutableList<String>,
        name: String,
        values: Map<*, *>,
        style: String,
        explode: Boolean,
        allowReserved: Boolean,
    ) {
        val serialized = mutableListOf<String>()
        values.forEach { (key, value) ->
            if (value == null) return@forEach
            if (style == "form" && explode) {
                pairs += urlEncode(key.toString()) + "=" + encodeQueryValue(value.toString(), allowReserved)
            } else {
                serialized += key.toString()
                serialized += value.toString()
            }
        }
        if (serialized.isNotEmpty()) {
            pairs += urlEncode(name) + "=" + encodeQueryValue(serialized.joinToString(","), allowReserved)
        }
    }

    private fun appendDeepObjectParameter(pairs: MutableList<String>, name: String, values: Map<*, *>, allowReserved: Boolean) {
        values.forEach { (key, value) ->
            if (value != null) {
                pairs += urlEncode("$name[$key]") + "=" + encodeQueryValue(value.toString(), allowReserved)
            }
        }
    }

    private fun encodeQueryValue(value: String, allowReserved: Boolean): String {
        var encoded = urlEncode(value)
        if (!allowReserved) return encoded
        mapOf(
            "%3A" to ":", "%2F" to "/", "%3F" to "?", "%23" to "#",
            "%5B" to "[", "%5D" to "]", "%40" to "@", "%21" to "!",
            "%24" to "$", "%26" to "&", "%27" to "'", "%28" to "(",
            "%29" to ")", "%2A" to "*", "%2B" to "+", "%2C" to ",",
            "%3B" to ";", "%3D" to "=",
        ).forEach { (escaped, reserved) -> encoded = encoded.replace(escaped, reserved) }
        return encoded
    }

    private fun urlEncode(value: String): String {
        return java.net.URLEncoder.encode(value, java.nio.charset.StandardCharsets.UTF_8)
    }

    private data class HeaderParameterSpec(val value: Any?, val style: String, val explode: Boolean, val contentType: String?)

    private val headerObjectMapper = ObjectMapper().registerKotlinModule()

    private fun buildRequestHeaders(headers: Map<String, HeaderParameterSpec>, cookies: Map<String, HeaderParameterSpec>): Map<String, String>? {
        val requestHeaders = linkedMapOf<String, String>()
        headers.forEach { (name, parameter) ->
            serializeParameterValue(parameter)?.let { requestHeaders[name] = it }
        }

        val cookieHeader = buildCookieHeader(cookies)
        if (cookieHeader.isNotEmpty()) {
            requestHeaders["Cookie"] = requestHeaders["Cookie"]?.let { "$it; $cookieHeader" } ?: cookieHeader
        }

        return requestHeaders.takeIf { it.isNotEmpty() }
    }

    private fun buildCookieHeader(cookies: Map<String, HeaderParameterSpec>): String {
        return cookies.mapNotNull { (name, parameter) ->
            serializeParameterValue(parameter)?.let {
                java.net.URLEncoder.encode(name, java.nio.charset.StandardCharsets.UTF_8) + "=" +
                    java.net.URLEncoder.encode(it, java.nio.charset.StandardCharsets.UTF_8)
            }
        }.joinToString("; ")
    }

    private fun serializeParameterValue(parameter: HeaderParameterSpec?): String? {
        val value = parameter?.value ?: return null
        if (!parameter.contentType.isNullOrBlank()) {
            return headerObjectMapper.writeValueAsString(value)
        }
        return when (value) {
            is Iterable<*> -> value.mapNotNull { it?.toString() }.joinToString(",")
            is Map<*, *> -> value.mapNotNull { (key, item) ->
                if (item == null) {
                    null
                } else if (parameter.explode) {
                    "$key=$item"
                } else {
                    listOf(key.toString(), item.toString()).joinToString(",")
                }
            }.joinToString(",")
            else -> value.toString()
        }
    }
}
