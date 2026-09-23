import Foundation

public class ClusterApi {
    private let client: HttpClient
    
    public init(client: HttpClient) {
        self.client = client
    }

    /// List Web Server clusters
    public func clustersList(page: Int? = nil, pageSize: Int? = nil) async throws -> ClustersListResponse? {
        let query = buildQueryString([
            QueryParameterSpec(name: "page", value: page, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "page_size", value: pageSize, style: "form", explode: true, allowReserved: false, contentType: nil)
        ])
        return try await client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters"), query), responseType: ClustersListResponse.self)
    }

    /// Create a Web Server cluster
    public func clustersCreate(body: CreateClusterRequest, idempotencyKey: String) async throws -> ClustersCreateResponse201? {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        return try await client.post(ApiPaths.backendPath("/clusters"), body: body, params: nil, headers: requestHeaders, contentType: "application/json", responseType: ClustersCreateResponse201.self)
    }

    /// Retrieve a Web Server cluster
    public func clustersRetrieve(clusterId: String) async throws -> ClustersRetrieveResponse? {
        return try await client.get(ApiPaths.backendPath("/clusters/\(serializePathParameter(clusterId, PathParameterSpec(name: "clusterId", style: "simple", explode: false)))"), responseType: ClustersRetrieveResponse.self)
    }

    /// Update a Web Server cluster
    public func clustersUpdate(clusterId: String, body: UpdateClusterRequest, idempotencyKey: String) async throws -> ClustersUpdateResponse? {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        return try await client.patch(ApiPaths.backendPath("/clusters/\(serializePathParameter(clusterId, PathParameterSpec(name: "clusterId", style: "simple", explode: false)))"), body: body, params: nil, headers: requestHeaders, contentType: "application/json", responseType: ClustersUpdateResponse.self)
    }

    /// Delete an empty Web Server cluster
    public func clustersDelete(clusterId: String, idempotencyKey: String) async throws -> Void {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        _ = try await client.delete(ApiPaths.backendPath("/clusters/\(serializePathParameter(clusterId, PathParameterSpec(name: "clusterId", style: "simple", explode: false)))"), params: nil, headers: requestHeaders)
    }

    /// Publish a desired-state revision to every instance of the cluster
    public func clustersSync(clusterId: String, body: PublishClusterSyncRequest) async throws -> ClustersSyncResponse? {
        return try await client.post(ApiPaths.backendPath("/clusters/\(serializePathParameter(clusterId, PathParameterSpec(name: "clusterId", style: "simple", explode: false)))/sync"), body: body, params: nil, headers: nil, contentType: "application/json", responseType: ClustersSyncResponse.self)
    }

    /// List cluster hosts with system and network identity
    public func clustersHostsList(pageSize: Int? = nil, cursor: String? = nil, clusterId: String? = nil, status: Int? = nil) async throws -> ClustersHostsListResponse? {
        let query = buildQueryString([
            QueryParameterSpec(name: "page_size", value: pageSize, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "cursor", value: cursor, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "cluster_id", value: clusterId, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "status", value: status, style: "form", explode: true, allowReserved: false, contentType: nil)
        ])
        return try await client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/hosts"), query), responseType: ClustersHostsListResponse.self)
    }

    /// Retrieve a cluster host
    public func clustersHostsRetrieve(hostId: String) async throws -> ClustersHostsRetrieveResponse? {
        return try await client.get(ApiPaths.backendPath("/clusters/hosts/\(serializePathParameter(hostId, PathParameterSpec(name: "hostId", style: "simple", explode: false)))"), responseType: ClustersHostsRetrieveResponse.self)
    }

    /// Rename a host or reassign it to another cluster
    public func clustersHostsUpdate(hostId: String, body: UpdateClusterHostRequest, idempotencyKey: String) async throws -> ClustersHostsUpdateResponse? {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        return try await client.patch(ApiPaths.backendPath("/clusters/hosts/\(serializePathParameter(hostId, PathParameterSpec(name: "hostId", style: "simple", explode: false)))"), body: body, params: nil, headers: requestHeaders, contentType: "application/json", responseType: ClustersHostsUpdateResponse.self)
    }

    /// Remove an instance-free host from the cluster inventory
    public func clustersHostsDelete(hostId: String, idempotencyKey: String) async throws -> Void {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        _ = try await client.delete(ApiPaths.backendPath("/clusters/hosts/\(serializePathParameter(hostId, PathParameterSpec(name: "hostId", style: "simple", explode: false)))"), params: nil, headers: requestHeaders)
    }

    /// List webserver process instances with liveness state
    public func clustersInstancesList(pageSize: Int? = nil, cursor: String? = nil, clusterId: String? = nil, hostId: String? = nil, status: Int? = nil, healthState: String? = nil) async throws -> ClustersInstancesListResponse? {
        let query = buildQueryString([
            QueryParameterSpec(name: "page_size", value: pageSize, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "cursor", value: cursor, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "cluster_id", value: clusterId, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "host_id", value: hostId, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "status", value: status, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "health_state", value: healthState, style: "form", explode: true, allowReserved: false, contentType: nil)
        ])
        return try await client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances"), query), responseType: ClustersInstancesListResponse.self)
    }

    /// Retrieve a webserver process instance
    public func clustersInstancesRetrieve(instanceId: String) async throws -> ClustersInstancesRetrieveResponse? {
        return try await client.get(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))"), responseType: ClustersInstancesRetrieveResponse.self)
    }

    /// Update an instance display name, status, or advertised endpoint
    public func clustersInstancesUpdate(instanceId: String, body: UpdateClusterInstanceRequest, idempotencyKey: String) async throws -> ClustersInstancesUpdateResponse? {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        return try await client.patch(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))"), body: body, params: nil, headers: requestHeaders, contentType: "application/json", responseType: ClustersInstancesUpdateResponse.self)
    }

    /// Unregister a webserver process instance
    public func clustersInstancesDelete(instanceId: String, idempotencyKey: String) async throws -> Void {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        _ = try await client.delete(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))"), params: nil, headers: requestHeaders)
    }

    /// List cluster lifecycle events
    public func clustersEventsList(pageSize: Int? = nil, cursor: String? = nil, clusterId: String? = nil, severity: String? = nil) async throws -> ClustersEventsListResponse? {
        let query = buildQueryString([
            QueryParameterSpec(name: "page_size", value: pageSize, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "cursor", value: cursor, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "cluster_id", value: clusterId, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "severity", value: severity, style: "form", explode: true, allowReserved: false, contentType: nil)
        ])
        return try await client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/events"), query), responseType: ClustersEventsListResponse.self)
    }

    /// Retrieve the cluster health overview for status polling
    public func clustersOverviewRetrieve() async throws -> ClustersOverviewRetrieveResponse? {
        return try await client.get(ApiPaths.backendPath("/clusters/overview"), responseType: ClustersOverviewRetrieveResponse.self)
    }

    /// List one instance's stored heartbeat samples
    public func clustersInstancesHeartbeatsList(instanceId: String, pageSize: Int? = nil, cursor: String? = nil) async throws -> ClustersInstancesHeartbeatsListResponse? {
        let query = buildQueryString([
            QueryParameterSpec(name: "page_size", value: pageSize, style: "form", explode: true, allowReserved: false, contentType: nil),
            QueryParameterSpec(name: "cursor", value: cursor, style: "form", explode: true, allowReserved: false, contentType: nil)
        ])
        return try await client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))/heartbeats"), query), responseType: ClustersInstancesHeartbeatsListResponse.self)
    }

    /// List one instance's heartbeat metric samples for trend charts
    public func clustersInstancesMetricsList(instanceId: String, limit: Int? = nil) async throws -> ClustersInstancesMetricsListResponse? {
        let query = buildQueryString([
            QueryParameterSpec(name: "limit", value: limit, style: "form", explode: true, allowReserved: false, contentType: nil)
        ])
        return try await client.get(ApiPaths.appendQueryString(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))/metrics/history"), query), responseType: ClustersInstancesMetricsListResponse.self)
    }

    /// Probe one instance's connectivity and record the outcome
    public func clustersInstancesProbe(instanceId: String, body: ProbeClusterInstanceRequest? = nil) async throws -> ClustersInstancesProbeResponse? {
        return try await client.post(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))/probe"), body: body, params: nil, headers: nil, contentType: "application/json", responseType: ClustersInstancesProbeResponse.self)
    }

    /// Gracefully drain one instance out of routing
    public func clustersInstancesDrain(instanceId: String) async throws -> ClustersInstancesDrainResponse? {
        return try await client.post(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))/drain"), body: nil, responseType: ClustersInstancesDrainResponse.self)
    }

    /// Clear the drain flag and restore routing participation
    public func clustersInstancesUndrain(instanceId: String) async throws -> ClustersInstancesUndrainResponse? {
        return try await client.post(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))/undrain"), body: nil, responseType: ClustersInstancesUndrainResponse.self)
    }

    /// Cordon one instance out of routing without draining it
    public func clustersInstancesCordon(instanceId: String) async throws -> ClustersInstancesCordonResponse? {
        return try await client.post(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))/cordon"), body: nil, responseType: ClustersInstancesCordonResponse.self)
    }

    /// Uncordon one instance back into routing
    public func clustersInstancesUncordon(instanceId: String) async throws -> ClustersInstancesUncordonResponse? {
        return try await client.post(ApiPaths.backendPath("/clusters/instances/\(serializePathParameter(instanceId, PathParameterSpec(name: "instanceId", style: "simple", explode: false)))/uncordon"), body: nil, responseType: ClustersInstancesUncordonResponse.self)
    }

    /// Enqueue a peer message to one instance or broadcast to online members
    public func clustersMessagesCreate(body: EnqueueClusterPeerMessagesRequest, idempotencyKey: String) async throws -> ClustersMessagesCreateResponse201? {
        let requestHeaders = buildRequestHeaders(
            [
                "Idempotency-Key": HeaderParameterSpec(value: idempotencyKey, style: "simple", explode: false, contentType: nil),
            ],
            [:]
        )
        return try await client.post(ApiPaths.backendPath("/clusters/messages"), body: body, params: nil, headers: requestHeaders, contentType: "application/json", responseType: ClustersMessagesCreateResponse201.self)
    }

    private struct PathParameterSpec {
        let name: String
        let style: String
        let explode: Bool
    }

    private func serializePathParameter(_ value: Any?, _ spec: PathParameterSpec) -> String {
        guard let value else { return "" }
        let style = spec.style.isEmpty ? "simple" : spec.style
        if let array = value as? [Any] {
            return serializePathArray(spec.name, array, style, spec.explode)
        }
        if let object = value as? [String: Any] {
            return serializePathObject(spec.name, object, style, spec.explode)
        }
        return pathPrimitivePrefix(spec.name, style) + pathEncode(String(describing: value))
    }

    private func serializePathArray(_ name: String, _ values: [Any], _ style: String, _ explode: Bool) -> String {
        let serialized = values.map { pathEncode(String(describing: $0)) }
        if serialized.isEmpty { return pathPrefix(name, style) }
        if style == "matrix" {
            if explode {
                return serialized.map { ";\(name)=\($0)" }.joined()
            }
            return ";\(name)=" + serialized.joined(separator: ",")
        }
        let separator = explode ? "." : ","
        return pathPrefix(name, style) + serialized.joined(separator: separator)
    }

    private func serializePathObject(_ name: String, _ values: [String: Any], _ style: String, _ explode: Bool) -> String {
        var entries: [String] = []
        var exploded: [String] = []
        for (key, value) in values {
            let escapedKey = pathEncode(key)
            let escapedValue = pathEncode(String(describing: value))
            if explode {
                if style == "matrix" {
                    exploded.append(";\(escapedKey)=\(escapedValue)")
                } else {
                    exploded.append("\(escapedKey)=\(escapedValue)")
                }
            } else {
                entries.append(escapedKey)
                entries.append(escapedValue)
            }
        }
        if style == "matrix" {
            if explode {
                return exploded.joined()
            }
            return ";\(name)=" + entries.joined(separator: ",")
        }
        if explode {
            let separator = style == "label" ? "." : ","
            return pathPrefix(name, style) + exploded.joined(separator: separator)
        }
        return pathPrefix(name, style) + entries.joined(separator: ",")
    }

    private func pathPrefix(_ name: String, _ style: String) -> String {
        if style == "label" { return "." }
        if style == "matrix" { return ";\(name)" }
        return ""
    }

    private func pathPrimitivePrefix(_ name: String, _ style: String) -> String {
        style == "matrix" ? ";\(name)=" : pathPrefix(name, style)
    }

    private func pathEncode(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? value
    }

    private struct QueryParameterSpec {
        let name: String
        let value: Any?
        let style: String
        let explode: Bool
        let allowReserved: Bool
        let contentType: String?
    }

    private func buildQueryString(_ parameters: [QueryParameterSpec]) -> String {
        var pairs: [String] = []
        for parameter in parameters {
            appendSerializedParameter(&pairs, parameter)
        }
        return pairs.joined(separator: "&")
    }

    private func appendSerializedParameter(_ pairs: inout [String], _ parameter: QueryParameterSpec) {
        guard let value = parameter.value else { return }
        if let contentType = parameter.contentType, !contentType.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let data = (try? JSONSerialization.data(withJSONObject: value, options: [])) ?? Data(String(describing: value).utf8)
            let json = String(data: data, encoding: .utf8) ?? String(describing: value)
            pairs.append("\(urlEncode(parameter.name))=\(encodeQueryValue(json, allowReserved: parameter.allowReserved))")
            return
        }

        let style = parameter.style.isEmpty ? "form" : parameter.style
        if style == "deepObject", let object = value as? [String: Any] {
            appendDeepObjectParameter(&pairs, name: parameter.name, values: object, allowReserved: parameter.allowReserved)
        } else if let array = value as? [Any] {
            appendArrayParameter(&pairs, name: parameter.name, values: array, style: style, explode: parameter.explode, allowReserved: parameter.allowReserved)
        } else if let object = value as? [String: Any] {
            appendObjectParameter(&pairs, name: parameter.name, values: object, style: style, explode: parameter.explode, allowReserved: parameter.allowReserved)
        } else {
            pairs.append("\(urlEncode(parameter.name))=\(encodeQueryValue(String(describing: value), allowReserved: parameter.allowReserved))")
        }
    }

    private func appendArrayParameter(
        _ pairs: inout [String],
        name: String,
        values: [Any],
        style: String,
        explode: Bool,
        allowReserved: Bool
    ) {
        let serialized = values.map { String(describing: $0) }
        guard !serialized.isEmpty else { return }
        if style == "form" && explode {
            for item in serialized {
                pairs.append("\(urlEncode(name))=\(encodeQueryValue(item, allowReserved: allowReserved))")
            }
            return
        }
        pairs.append("\(urlEncode(name))=\(encodeQueryValue(serialized.joined(separator: ","), allowReserved: allowReserved))")
    }

    private func appendObjectParameter(
        _ pairs: inout [String],
        name: String,
        values: [String: Any],
        style: String,
        explode: Bool,
        allowReserved: Bool
    ) {
        var serialized: [String] = []
        for (key, value) in values {
            if style == "form" && explode {
                pairs.append("\(urlEncode(key))=\(encodeQueryValue(String(describing: value), allowReserved: allowReserved))")
            } else {
                serialized.append(key)
                serialized.append(String(describing: value))
            }
        }
        if !serialized.isEmpty {
            pairs.append("\(urlEncode(name))=\(encodeQueryValue(serialized.joined(separator: ","), allowReserved: allowReserved))")
        }
    }

    private func appendDeepObjectParameter(_ pairs: inout [String], name: String, values: [String: Any], allowReserved: Bool) {
        for (key, value) in values {
            pairs.append("\(urlEncode("\(name)[\(key)]"))=\(encodeQueryValue(String(describing: value), allowReserved: allowReserved))")
        }
    }

    private func encodeQueryValue(_ value: String, allowReserved: Bool) -> String {
        var encoded = urlEncode(value)
        if !allowReserved { return encoded }
        [
            "%3A": ":", "%2F": "/", "%3F": "?", "%23": "#",
            "%5B": "[", "%5D": "]", "%40": "@", "%21": "!",
            "%24": "$", "%26": "&", "%27": "'", "%28": "(",
            "%29": ")", "%2A": "*", "%2B": "+", "%2C": ",",
            "%3B": ";", "%3D": "=",
        ].forEach { encoded = encoded.replacingOccurrences(of: $0.key, with: $0.value) }
        return encoded
    }

    private func urlEncode(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? value
    }

    private struct HeaderParameterSpec {
        let value: Any?
        let style: String
        let explode: Bool
        let contentType: String?
    }

    private func buildRequestHeaders(_ headers: [String: HeaderParameterSpec], _ cookies: [String: HeaderParameterSpec]) -> [String: String]? {
        var requestHeaders: [String: String] = [:]
        for (name, parameter) in headers {
            if let serialized = serializeParameterValue(parameter) {
                requestHeaders[name] = serialized
            }
        }

        if let cookieHeader = buildCookieHeader(cookies), !cookieHeader.isEmpty {
            requestHeaders["Cookie"] = requestHeaders["Cookie"].map { "\($0); \(cookieHeader)" } ?? cookieHeader
        }

        return requestHeaders.isEmpty ? nil : requestHeaders
    }

    private func buildCookieHeader(_ cookies: [String: HeaderParameterSpec]) -> String? {
        let pairs = cookies.compactMap { name, parameter -> String? in
            guard let serialized = serializeParameterValue(parameter) else { return nil }
            return "\(urlEncode(name))=\(urlEncode(serialized))"
        }
        return pairs.isEmpty ? nil : pairs.joined(separator: "; ")
    }

    private func serializeParameterValue(_ parameter: HeaderParameterSpec?) -> String? {
        guard let parameter, let value = parameter.value else { return nil }
        if let contentType = parameter.contentType, !contentType.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            if JSONSerialization.isValidJSONObject(value),
               let data = try? JSONSerialization.data(withJSONObject: value, options: []),
               let json = String(data: data, encoding: .utf8) {
                return json
            }
            return String(describing: value)
        }
        if let array = value as? [Any?] {
            return array.compactMap { $0.map { String(describing: $0) } }.joined(separator: ",")
        }
        if let object = value as? [String: Any] {
            var values: [String] = []
            for (key, item) in object {
                if parameter.explode {
                    values.append("\(key)=\(item)")
                } else {
                    values.append(key)
                    values.append(String(describing: item))
                }
            }
            return values.joined(separator: ",")
        }
        if let date = value as? Date {
            return ISO8601DateFormatter().string(from: date)
        }
        return String(describing: value)
    }

    private func urlEncode(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? value
    }
}
