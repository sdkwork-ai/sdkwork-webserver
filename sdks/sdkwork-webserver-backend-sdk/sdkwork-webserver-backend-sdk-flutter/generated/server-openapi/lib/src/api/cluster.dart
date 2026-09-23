import 'dart:convert';
import '../http/client.dart';
import '../models.dart';

import 'paths.dart';
import 'response_helpers.dart';


class ClusterApi {
  final HttpClient _client;

  ClusterApi(this._client);

  /// List Web Server clusters
  Future<ClustersListResponse?> clustersList([int? page, int? pageSize]) async {
    final query = buildQueryString([
      QueryParameterSpec('page', page, 'form', true, false, null),
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.backendPath('/clusters'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersListResponse.fromJson(map);
    })();
  }

  /// Create a Web Server cluster
  Future<ClustersCreateResponse201?> clustersCreate(CreateClusterRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.backendPath('/clusters'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersCreateResponse201.fromJson(map);
    })();
  }

  /// Retrieve a Web Server cluster
  Future<ClustersRetrieveResponse?> clustersRetrieve(String clusterId) async {
    final response = await _client.get(ApiPaths.backendPath('/clusters/${serializePathParameter(clusterId, const PathParameterSpec('clusterId', 'simple', false))}'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersRetrieveResponse.fromJson(map);
    })();
  }

  /// Update a Web Server cluster
  Future<ClustersUpdateResponse?> clustersUpdate(String clusterId, UpdateClusterRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.patch(ApiPaths.backendPath('/clusters/${serializePathParameter(clusterId, const PathParameterSpec('clusterId', 'simple', false))}'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersUpdateResponse.fromJson(map);
    })();
  }

  /// Delete an empty Web Server cluster
  Future<void> clustersDelete(String clusterId, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    await _client.delete(ApiPaths.backendPath('/clusters/${serializePathParameter(clusterId, const PathParameterSpec('clusterId', 'simple', false))}'), headers: requestHeaders);
  }

  /// Publish a desired-state revision to every instance of the cluster
  Future<ClustersSyncResponse?> clustersSync(String clusterId, PublishClusterSyncRequest body) async {
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.backendPath('/clusters/${serializePathParameter(clusterId, const PathParameterSpec('clusterId', 'simple', false))}/sync'), body: payload, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersSyncResponse.fromJson(map);
    })();
  }

  /// List cluster hosts with system and network identity
  Future<ClustersHostsListResponse?> clustersHostsList([int? pageSize, String? cursor, String? clusterId, int? status]) async {
    final query = buildQueryString([
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null),
      QueryParameterSpec('cursor', cursor, 'form', true, false, null),
      QueryParameterSpec('cluster_id', clusterId, 'form', true, false, null),
      QueryParameterSpec('status', status, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.backendPath('/clusters/hosts'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersHostsListResponse.fromJson(map);
    })();
  }

  /// Retrieve a cluster host
  Future<ClustersHostsRetrieveResponse?> clustersHostsRetrieve(String hostId) async {
    final response = await _client.get(ApiPaths.backendPath('/clusters/hosts/${serializePathParameter(hostId, const PathParameterSpec('hostId', 'simple', false))}'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersHostsRetrieveResponse.fromJson(map);
    })();
  }

  /// Rename a host or reassign it to another cluster
  Future<ClustersHostsUpdateResponse?> clustersHostsUpdate(String hostId, UpdateClusterHostRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.patch(ApiPaths.backendPath('/clusters/hosts/${serializePathParameter(hostId, const PathParameterSpec('hostId', 'simple', false))}'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersHostsUpdateResponse.fromJson(map);
    })();
  }

  /// Remove an instance-free host from the cluster inventory
  Future<void> clustersHostsDelete(String hostId, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    await _client.delete(ApiPaths.backendPath('/clusters/hosts/${serializePathParameter(hostId, const PathParameterSpec('hostId', 'simple', false))}'), headers: requestHeaders);
  }

  /// List webserver process instances with liveness state
  Future<ClustersInstancesListResponse?> clustersInstancesList([int? pageSize, String? cursor, String? clusterId, String? hostId, int? status, String? healthState]) async {
    final query = buildQueryString([
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null),
      QueryParameterSpec('cursor', cursor, 'form', true, false, null),
      QueryParameterSpec('cluster_id', clusterId, 'form', true, false, null),
      QueryParameterSpec('host_id', hostId, 'form', true, false, null),
      QueryParameterSpec('status', status, 'form', true, false, null),
      QueryParameterSpec('health_state', healthState, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.backendPath('/clusters/instances'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesListResponse.fromJson(map);
    })();
  }

  /// Retrieve a webserver process instance
  Future<ClustersInstancesRetrieveResponse?> clustersInstancesRetrieve(String instanceId) async {
    final response = await _client.get(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesRetrieveResponse.fromJson(map);
    })();
  }

  /// Update an instance display name, status, or advertised endpoint
  Future<ClustersInstancesUpdateResponse?> clustersInstancesUpdate(String instanceId, UpdateClusterInstanceRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.patch(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesUpdateResponse.fromJson(map);
    })();
  }

  /// Unregister a webserver process instance
  Future<void> clustersInstancesDelete(String instanceId, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    await _client.delete(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}'), headers: requestHeaders);
  }

  /// List cluster lifecycle events
  Future<ClustersEventsListResponse?> clustersEventsList([int? pageSize, String? cursor, String? clusterId, String? severity]) async {
    final query = buildQueryString([
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null),
      QueryParameterSpec('cursor', cursor, 'form', true, false, null),
      QueryParameterSpec('cluster_id', clusterId, 'form', true, false, null),
      QueryParameterSpec('severity', severity, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.backendPath('/clusters/events'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersEventsListResponse.fromJson(map);
    })();
  }

  /// Retrieve the cluster health overview for status polling
  Future<ClustersOverviewRetrieveResponse?> clustersOverviewRetrieve() async {
    final response = await _client.get(ApiPaths.backendPath('/clusters/overview'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersOverviewRetrieveResponse.fromJson(map);
    })();
  }

  /// List one instance's stored heartbeat samples
  Future<ClustersInstancesHeartbeatsListResponse?> clustersInstancesHeartbeatsList(String instanceId, [int? pageSize, String? cursor]) async {
    final query = buildQueryString([
      QueryParameterSpec('page_size', pageSize, 'form', true, false, null),
      QueryParameterSpec('cursor', cursor, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}/heartbeats'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesHeartbeatsListResponse.fromJson(map);
    })();
  }

  /// List one instance's heartbeat metric samples for trend charts
  Future<ClustersInstancesMetricsListResponse?> clustersInstancesMetricsList(String instanceId, [int? limit]) async {
    final query = buildQueryString([
      QueryParameterSpec('limit', limit, 'form', true, false, null)
    ]);
    final response = await _client.get(ApiPaths.appendQueryString(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}/metrics/history'), query));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesMetricsListResponse.fromJson(map);
    })();
  }

  /// Probe one instance's connectivity and record the outcome
  Future<ClustersInstancesProbeResponse?> clustersInstancesProbe(String instanceId, [ProbeClusterInstanceRequest? body]) async {
    final payload = body?.toJson();
    final response = await _client.post(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}/probe'), body: payload, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesProbeResponse.fromJson(map);
    })();
  }

  /// Gracefully drain one instance out of routing
  Future<ClustersInstancesDrainResponse?> clustersInstancesDrain(String instanceId) async {
    final response = await _client.post(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}/drain'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesDrainResponse.fromJson(map);
    })();
  }

  /// Clear the drain flag and restore routing participation
  Future<ClustersInstancesUndrainResponse?> clustersInstancesUndrain(String instanceId) async {
    final response = await _client.post(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}/undrain'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesUndrainResponse.fromJson(map);
    })();
  }

  /// Cordon one instance out of routing without draining it
  Future<ClustersInstancesCordonResponse?> clustersInstancesCordon(String instanceId) async {
    final response = await _client.post(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}/cordon'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesCordonResponse.fromJson(map);
    })();
  }

  /// Uncordon one instance back into routing
  Future<ClustersInstancesUncordonResponse?> clustersInstancesUncordon(String instanceId) async {
    final response = await _client.post(ApiPaths.backendPath('/clusters/instances/${serializePathParameter(instanceId, const PathParameterSpec('instanceId', 'simple', false))}/uncordon'));
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersInstancesUncordonResponse.fromJson(map);
    })();
  }

  /// Enqueue a peer message to one instance or broadcast to online members
  Future<ClustersMessagesCreateResponse201?> clustersMessagesCreate(EnqueueClusterPeerMessagesRequest body, String idempotencyKey) async {
    final requestHeaders = buildRequestHeaders(
      <String, HeaderParameterSpec>{
        'Idempotency-Key': HeaderParameterSpec(idempotencyKey, 'simple', false, null),
      },
      <String, HeaderParameterSpec>{},
    );
    final payload = body.toJson();
    final response = await _client.post(ApiPaths.backendPath('/clusters/messages'), body: payload, headers: requestHeaders, contentType: 'application/json');
    return (() {
      final map = sdkworkResponseAsMap(response);
      return map == null ? null : ClustersMessagesCreateResponse201.fromJson(map);
    })();
  }
}

class PathParameterSpec {
  final String name;
  final String style;
  final bool explode;

  const PathParameterSpec(this.name, this.style, this.explode);
}

String serializePathParameter(dynamic value, PathParameterSpec spec) {
  if (value == null) return '';
  final style = spec.style.trim().isEmpty ? 'simple' : spec.style;
  if (value is Iterable) {
    return serializePathArray(spec.name, value, style, spec.explode);
  }
  if (value is Map) {
    return serializePathObject(spec.name, value, style, spec.explode);
  }
  return pathPrimitivePrefix(spec.name, style) + Uri.encodeComponent(value.toString());
}

String serializePathArray(String name, Iterable values, String style, bool explode) {
  final serialized = values.where((item) => item != null).map((item) => Uri.encodeComponent(item.toString())).toList();
  if (serialized.isEmpty) return pathPrefix(name, style);
  if (style == 'matrix') {
    if (explode) {
      return serialized.map((item) => ';$name=$item').join();
    }
    return ';$name=${serialized.join(',')}';
  }
  final separator = explode ? '.' : ',';
  return pathPrefix(name, style) + serialized.join(separator);
}

String serializePathObject(String name, Map values, String style, bool explode) {
  final entries = <String>[];
  final exploded = <String>[];
  values.forEach((key, value) {
    if (value == null) return;
    final escapedKey = Uri.encodeComponent(key.toString());
    final escapedValue = Uri.encodeComponent(value.toString());
    if (explode) {
      if (style == 'matrix') {
        exploded.add(';$escapedKey=$escapedValue');
      } else {
        exploded.add('$escapedKey=$escapedValue');
      }
    } else {
      entries.add(escapedKey);
      entries.add(escapedValue);
    }
  });
  if (style == 'matrix') {
    if (explode) return exploded.join();
    return ';$name=${entries.join(',')}';
  }
  if (explode) {
    final separator = style == 'label' ? '.' : ',';
    return pathPrefix(name, style) + exploded.join(separator);
  }
  return pathPrefix(name, style) + entries.join(',');
}

String pathPrefix(String name, String style) {
  if (style == 'label') return '.';
  if (style == 'matrix') return ';$name';
  return '';
}

String pathPrimitivePrefix(String name, String style) {
  return style == 'matrix' ? ';$name=' : pathPrefix(name, style);
}
class QueryParameterSpec {
  final String name;
  final dynamic value;
  final String style;
  final bool explode;
  final bool allowReserved;
  final String? contentType;

  const QueryParameterSpec(
    this.name,
    this.value,
    this.style,
    this.explode,
    this.allowReserved,
    this.contentType,
  );
}

String buildQueryString(List<QueryParameterSpec> parameters) {
  final pairs = <String>[];
  for (final parameter in parameters) {
    appendSerializedParameter(pairs, parameter);
  }
  return pairs.join('&');
}

void appendSerializedParameter(List<String> pairs, QueryParameterSpec parameter) {
  final value = parameter.value;
  if (value == null) return;

  final contentType = parameter.contentType;
  if (contentType != null && contentType.trim().isNotEmpty) {
    pairs.add('${urlEncode(parameter.name)}=${encodeQueryValue(jsonEncode(value), parameter.allowReserved)}');
    return;
  }

  final style = parameter.style.trim().isEmpty ? 'form' : parameter.style;
  if (style == 'deepObject' && value is Map) {
    appendDeepObjectParameter(pairs, parameter.name, value, parameter.allowReserved);
    return;
  }
  if (value is Iterable) {
    appendArrayParameter(pairs, parameter.name, value, style, parameter.explode, parameter.allowReserved);
    return;
  }
  if (value is Map) {
    appendObjectParameter(pairs, parameter.name, value, style, parameter.explode, parameter.allowReserved);
    return;
  }
  pairs.add('${urlEncode(parameter.name)}=${encodeQueryValue(value.toString(), parameter.allowReserved)}');
}

void appendArrayParameter(
  List<String> pairs,
  String name,
  Iterable values,
  String style,
  bool explode,
  bool allowReserved,
) {
  final serialized = values.where((item) => item != null).map((item) => item.toString()).toList();
  if (serialized.isEmpty) return;
  if (style == 'form' && explode) {
    for (final item in serialized) {
      pairs.add('${urlEncode(name)}=${encodeQueryValue(item, allowReserved)}');
    }
    return;
  }
  pairs.add('${urlEncode(name)}=${encodeQueryValue(serialized.join(','), allowReserved)}');
}

void appendObjectParameter(
  List<String> pairs,
  String name,
  Map values,
  String style,
  bool explode,
  bool allowReserved,
) {
  final serialized = <String>[];
  values.forEach((key, value) {
    if (value == null) return;
    if (style == 'form' && explode) {
      pairs.add('${urlEncode(key.toString())}=${encodeQueryValue(value.toString(), allowReserved)}');
      return;
    }
    serialized.add(key.toString());
    serialized.add(value.toString());
  });
  if (serialized.isNotEmpty) {
    pairs.add('${urlEncode(name)}=${encodeQueryValue(serialized.join(','), allowReserved)}');
  }
}

void appendDeepObjectParameter(List<String> pairs, String name, Map values, bool allowReserved) {
  values.forEach((key, value) {
    if (value != null) {
      pairs.add('${urlEncode('$name[$key]')}=${encodeQueryValue(value.toString(), allowReserved)}');
    }
  });
}

String encodeQueryValue(String value, bool allowReserved) {
  var encoded = urlEncode(value);
  if (!allowReserved) return encoded;
  const replacements = <String, String>{
    '%3A': ':',
    '%2F': '/',
    '%3F': '?',
    '%23': '#',
    '%5B': '[',
    '%5D': ']',
    '%40': '@',
    '%21': '!',
    '%24': r'$',
    '%26': '&',
    '%27': "'",
    '%28': '(',
    '%29': ')',
    '%2A': '*',
    '%2B': '+',
    '%2C': ',',
    '%3B': ';',
    '%3D': '=',
  };
  replacements.forEach((escaped, reserved) {
    encoded = encoded.replaceAll(escaped, reserved);
  });
  return encoded;
}

String urlEncode(String value) => Uri.encodeQueryComponent(value);
class HeaderParameterSpec {
  final dynamic value;
  final String style;
  final bool explode;
  final String? contentType;

  HeaderParameterSpec(this.value, this.style, this.explode, this.contentType);
}

Map<String, String>? buildRequestHeaders(
  Map<String, HeaderParameterSpec> headers, [
  Map<String, HeaderParameterSpec> cookies = const {},
]) {
  final requestHeaders = <String, String>{};

  headers.forEach((name, parameter) {
    final serialized = serializeParameterValue(parameter);
    if (serialized != null) {
      requestHeaders[name] = serialized;
    }
  });

  final cookieHeader = buildCookieHeader(cookies);
  if (cookieHeader != null && cookieHeader.isNotEmpty) {
    requestHeaders['Cookie'] = requestHeaders.containsKey('Cookie')
        ? '${requestHeaders['Cookie']}; $cookieHeader'
        : cookieHeader;
  }

  return requestHeaders.isEmpty ? null : requestHeaders;
}

String? buildCookieHeader(Map<String, HeaderParameterSpec> cookies) {
  final pairs = <String>[];
  cookies.forEach((name, parameter) {
    final serialized = serializeParameterValue(parameter);
    if (serialized != null) {
      pairs.add('${Uri.encodeComponent(name)}=${Uri.encodeComponent(serialized)}');
    }
  });
  return pairs.isEmpty ? null : pairs.join('; ');
}

String? serializeParameterValue(HeaderParameterSpec? parameter) {
  final value = parameter?.value;
  if (value == null) return null;
  if (parameter!.contentType != null && parameter.contentType!.trim().isNotEmpty) {
    return jsonEncode(value);
  }
  if (value is DateTime) return value.toIso8601String();
  if (value is Iterable) {
    return value
        .where((item) => item != null)
        .map((item) => item.toString())
        .whereType<String>()
        .join(',');
  }
  if (value is Map) {
    final serialized = <String>[];
    value.forEach((key, item) {
      if (item == null) return;
      if (parameter.explode) {
        serialized.add('$key=$item');
      } else {
        serialized.add(key.toString());
        serialized.add(item.toString());
      }
    });
    return serialized.join(',');
  }
  return value.toString();
}
