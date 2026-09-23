from typing import Any, Dict, List, Optional
from ..http_client import HttpClient
from ..models import ClustersCreateResponse201, ClustersEventsListResponse, ClustersHostsListResponse, ClustersHostsRetrieveResponse, ClustersHostsUpdateResponse, ClustersInstancesCordonResponse, ClustersInstancesDrainResponse, ClustersInstancesHeartbeatsListResponse, ClustersInstancesListResponse, ClustersInstancesMetricsListResponse, ClustersInstancesProbeResponse, ClustersInstancesRetrieveResponse, ClustersInstancesUncordonResponse, ClustersInstancesUndrainResponse, ClustersInstancesUpdateResponse, ClustersListResponse, ClustersMessagesCreateResponse201, ClustersOverviewRetrieveResponse, ClustersRetrieveResponse, ClustersSyncResponse, ClustersUpdateResponse, CreateClusterRequest, EnqueueClusterPeerMessagesRequest, ProbeClusterInstanceRequest, PublishClusterSyncRequest, UpdateClusterHostRequest, UpdateClusterInstanceRequest, UpdateClusterRequest

def _append_query_string(path: str, raw_query_string: str) -> str:
    query = raw_query_string.lstrip('?')
    if not query:
        return path
    separator = '&' if '?' in path else '?'
    return f"{path}{separator}{query}"

def serialize_path_parameter(value: Any, spec: Dict[str, Any]) -> str:
    if value is None:
        return ''

    style = str(spec.get('style') or 'simple')
    name = str(spec.get('name') or '')
    explode = bool(spec.get('explode'))
    if isinstance(value, (list, tuple)):
        return serialize_path_array(name, value, style, explode)
    if isinstance(value, dict):
        return serialize_path_object(name, value, style, explode)
    return path_prefix(name, style) + encode_path_value(serialize_path_primitive(value))


def serialize_path_array(name: str, values: Any, style: str, explode: bool) -> str:
    serialized = [encode_path_value(serialize_path_primitive(item)) for item in values if item is not None]
    if not serialized:
        return path_prefix(name, style)
    if style == 'matrix':
        return ''.join(f";{name}={item}" for item in serialized) if explode else f";{name}={','.join(serialized)}"
    return path_prefix(name, style) + ('.' if explode else ',').join(serialized)


def serialize_path_object(name: str, value: Dict[str, Any], style: str, explode: bool) -> str:
    entries = [(key, entry_value) for key, entry_value in value.items() if entry_value is not None]
    if not entries:
        return path_prefix(name, style)
    if style == 'matrix':
        if explode:
            return ''.join(f";{encode_path_value(str(key))}={encode_path_value(serialize_path_primitive(entry_value))}" for key, entry_value in entries)
        serialized = ','.join(item for key, entry_value in entries for item in (encode_path_value(str(key)), encode_path_value(serialize_path_primitive(entry_value))))
        return f";{name}={serialized}"
    if explode:
        separator = '.' if style == 'label' else ','
        serialized = separator.join(f"{encode_path_value(str(key))}={encode_path_value(serialize_path_primitive(entry_value))}" for key, entry_value in entries)
    else:
        serialized = ','.join(item for key, entry_value in entries for item in (encode_path_value(str(key)), encode_path_value(serialize_path_primitive(entry_value))))
    return path_prefix(name, style) + serialized


def path_prefix(name: str, style: str) -> str:
    if style == 'label':
        return '.'
    if style == 'matrix':
        return f";{name}"
    return ''


def encode_path_value(value: str) -> str:
    from urllib.parse import quote

    return quote(value, safe='')


def serialize_path_primitive(value: Any) -> str:
    if isinstance(value, dict):
        import json

        return json.dumps(value, separators=(',', ':'))
    return str(value)


def build_query_string(parameters: List[Dict[str, Any]]) -> str:
    pairs: List[str] = []
    for parameter in parameters:
        append_serialized_parameter(pairs, parameter)
    return '&'.join(pairs)


def append_serialized_parameter(pairs: List[str], parameter: Dict[str, Any]) -> None:
    value = parameter.get('value')
    if value is None:
        return

    name = str(parameter.get('name') or '')
    allow_reserved = bool(parameter.get('allow_reserved'))
    content_type = parameter.get('content_type')
    if content_type:
        import json

        pairs.append(f"{encode_query_component(name)}={encode_query_value(json.dumps(value, separators=(',', ':')), allow_reserved)}")
        return

    style = str(parameter.get('style') or 'form')
    explode = bool(parameter.get('explode'))
    if style == 'deepObject':
        append_deep_object_parameter(pairs, name, value, allow_reserved)
        return
    if isinstance(value, (list, tuple)):
        append_array_parameter(pairs, name, value, style, explode, allow_reserved)
        return
    if isinstance(value, dict):
        append_object_parameter(pairs, name, value, style, explode, allow_reserved)
        return

    pairs.append(f"{encode_query_component(name)}={encode_query_value(serialize_primitive(value), allow_reserved)}")


def append_array_parameter(
    pairs: List[str],
    name: str,
    value: Any,
    style: str,
    explode: bool,
    allow_reserved: bool,
) -> None:
    values = [serialize_primitive(item) for item in value if item is not None]
    if not values:
        return

    if style == 'form' and explode:
        for item in values:
            pairs.append(f"{encode_query_component(name)}={encode_query_value(item, allow_reserved)}")
        return

    pairs.append(f"{encode_query_component(name)}={encode_query_value(','.join(values), allow_reserved)}")


def append_object_parameter(
    pairs: List[str],
    name: str,
    value: Dict[str, Any],
    style: str,
    explode: bool,
    allow_reserved: bool,
) -> None:
    entries = [(key, entry_value) for key, entry_value in value.items() if entry_value is not None]
    if not entries:
        return

    if style == 'form' and explode:
        for key, entry_value in entries:
            pairs.append(f"{encode_query_component(str(key))}={encode_query_value(serialize_primitive(entry_value), allow_reserved)}")
        return

    serialized = ','.join(
        item
        for key, entry_value in entries
        for item in (str(key), serialize_primitive(entry_value))
    )
    pairs.append(f"{encode_query_component(name)}={encode_query_value(serialized, allow_reserved)}")


def append_deep_object_parameter(pairs: List[str], name: str, value: Any, allow_reserved: bool) -> None:
    if not isinstance(value, dict):
        pairs.append(f"{encode_query_component(name)}={encode_query_value(serialize_primitive(value), allow_reserved)}")
        return

    for key, entry_value in value.items():
        if entry_value is None:
            continue
        pairs.append(f"{encode_query_component(f'{name}[{key}]')}={encode_query_value(serialize_primitive(entry_value), allow_reserved)}")


def serialize_primitive(value: Any) -> str:
    if isinstance(value, dict):
        import json

        return json.dumps(value, separators=(',', ':'))
    return str(value)


def encode_query_component(value: str) -> str:
    from urllib.parse import quote

    return quote(value, safe='')


def encode_query_value(value: str, allow_reserved: bool) -> str:
    from urllib.parse import quote

    return quote(value, safe=':/?#[]@!$&\'()*+,;=' if allow_reserved else '')

def build_request_headers(headers: Dict[str, Dict[str, Any]], cookies: Optional[Dict[str, Dict[str, Any]]] = None) -> Optional[Dict[str, str]]:
    request_headers: Dict[str, str] = {}
    for name, parameter in headers.items():
        serialized = serialize_parameter_value(parameter)
        if serialized is not None:
            request_headers[name] = serialized

    cookie_header = build_cookie_header(cookies or {})
    if cookie_header:
        request_headers['Cookie'] = (
            f"{request_headers['Cookie']}; {cookie_header}"
            if 'Cookie' in request_headers
            else cookie_header
        )

    return request_headers or None


def build_cookie_header(cookies: Dict[str, Dict[str, Any]]) -> Optional[str]:
    from urllib.parse import quote

    pairs: List[str] = []
    for name, parameter in cookies.items():
        serialized = serialize_parameter_value(parameter)
        if serialized is not None:
            pairs.append(f"{quote(str(name), safe='')}={quote(serialized, safe='')}")
    return '; '.join(pairs) if pairs else None


def serialize_parameter_value(parameter: Optional[Dict[str, Any]]) -> Optional[str]:
    value = None if parameter is None else parameter.get('value')
    if value is None:
        return None
    if parameter and parameter.get('content_type'):
        import json

        return json.dumps(value, separators=(',', ':'))
    if isinstance(value, (list, tuple)):
        return ','.join(serialize_header_primitive(item) for item in value if item is not None)
    if isinstance(value, dict):
        return serialize_header_object(value, bool(parameter and parameter.get('explode')))
    return serialize_header_primitive(value)


def serialize_header_object(value: Dict[str, Any], explode: bool) -> str:
    entries = [(key, entry_value) for key, entry_value in value.items() if entry_value is not None]
    if explode:
        return ','.join(f"{key}={serialize_header_primitive(entry_value)}" for key, entry_value in entries)
    return ','.join(item for key, entry_value in entries for item in (str(key), serialize_header_primitive(entry_value)))


def serialize_header_primitive(value: Any) -> str:
    return str(value)


class ClusterApi:
    """cluster clusters API client."""

    def __init__(self, client: HttpClient):
        self._client = client
        self.hosts = ClusterHostsApi(client)
        self.instances = ClusterInstancesApi(client)
        self.events = ClusterEventsApi(client)
        self.overview = ClusterOverviewApi(client)
        self.messages = ClusterMessagesApi(client)


    def list(self, page: Optional[int] = None, page_size: Optional[int] = None) -> ClustersListResponse:
        """List Web Server clusters"""
        query = build_query_string([
            {'name': 'page', 'value': page, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'page_size', 'value': page_size, 'style': 'form', 'explode': True, 'allow_reserved': False},
        ])
        return self._client.get(_append_query_string(f"/backend/v3/api/clusters", query))

    def create(self, body: CreateClusterRequest, idempotency_key: str) -> ClustersCreateResponse201:
        """Create a Web Server cluster"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.post(f"/backend/v3/api/clusters", json=body, headers=request_headers)

    def retrieve(self, cluster_id: str) -> ClustersRetrieveResponse:
        """Retrieve a Web Server cluster"""
        return self._client.get(f"/backend/v3/api/clusters/{serialize_path_parameter(cluster_id, {'name': 'clusterId', 'style': 'simple', 'explode': False})}")

    def update(self, cluster_id: str, body: UpdateClusterRequest, idempotency_key: str) -> ClustersUpdateResponse:
        """Update a Web Server cluster"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.patch(f"/backend/v3/api/clusters/{serialize_path_parameter(cluster_id, {'name': 'clusterId', 'style': 'simple', 'explode': False})}", json=body, headers=request_headers)

    def delete(self, cluster_id: str, idempotency_key: str) -> None:
        """Delete an empty Web Server cluster"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.delete(f"/backend/v3/api/clusters/{serialize_path_parameter(cluster_id, {'name': 'clusterId', 'style': 'simple', 'explode': False})}", headers=request_headers)

    def create_sync(self, cluster_id: str, body: PublishClusterSyncRequest) -> ClustersSyncResponse:
        """Publish a desired-state revision to every instance of the cluster"""
        return self._client.post(f"/backend/v3/api/clusters/{serialize_path_parameter(cluster_id, {'name': 'clusterId', 'style': 'simple', 'explode': False})}/sync", json=body)

class ClusterHostsApi:
    """cluster clusters.hosts API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def list(self, page_size: Optional[int] = None, cursor: Optional[str] = None, cluster_id: Optional[str] = None, status: Optional[int] = None) -> ClustersHostsListResponse:
        """List cluster hosts with system and network identity"""
        query = build_query_string([
            {'name': 'page_size', 'value': page_size, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'cursor', 'value': cursor, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'cluster_id', 'value': cluster_id, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'status', 'value': status, 'style': 'form', 'explode': True, 'allow_reserved': False},
        ])
        return self._client.get(_append_query_string(f"/backend/v3/api/clusters/hosts", query))

    def retrieve(self, host_id: str) -> ClustersHostsRetrieveResponse:
        """Retrieve a cluster host"""
        return self._client.get(f"/backend/v3/api/clusters/hosts/{serialize_path_parameter(host_id, {'name': 'hostId', 'style': 'simple', 'explode': False})}")

    def update(self, host_id: str, body: UpdateClusterHostRequest, idempotency_key: str) -> ClustersHostsUpdateResponse:
        """Rename a host or reassign it to another cluster"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.patch(f"/backend/v3/api/clusters/hosts/{serialize_path_parameter(host_id, {'name': 'hostId', 'style': 'simple', 'explode': False})}", json=body, headers=request_headers)

    def delete(self, host_id: str, idempotency_key: str) -> None:
        """Remove an instance-free host from the cluster inventory"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.delete(f"/backend/v3/api/clusters/hosts/{serialize_path_parameter(host_id, {'name': 'hostId', 'style': 'simple', 'explode': False})}", headers=request_headers)

class ClusterInstancesApi:
    """cluster clusters.instances API client."""

    def __init__(self, client: HttpClient):
        self._client = client
        self.heartbeats = ClusterInstancesHeartbeatsApi(client)
        self.metrics = ClusterInstancesMetricsApi(client)


    def list(self, page_size: Optional[int] = None, cursor: Optional[str] = None, cluster_id: Optional[str] = None, host_id: Optional[str] = None, status: Optional[int] = None, health_state: Optional[str] = None) -> ClustersInstancesListResponse:
        """List webserver process instances with liveness state"""
        query = build_query_string([
            {'name': 'page_size', 'value': page_size, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'cursor', 'value': cursor, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'cluster_id', 'value': cluster_id, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'host_id', 'value': host_id, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'status', 'value': status, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'health_state', 'value': health_state, 'style': 'form', 'explode': True, 'allow_reserved': False},
        ])
        return self._client.get(_append_query_string(f"/backend/v3/api/clusters/instances", query))

    def retrieve(self, instance_id: str) -> ClustersInstancesRetrieveResponse:
        """Retrieve a webserver process instance"""
        return self._client.get(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}")

    def update(self, instance_id: str, body: UpdateClusterInstanceRequest, idempotency_key: str) -> ClustersInstancesUpdateResponse:
        """Update an instance display name, status, or advertised endpoint"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.patch(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}", json=body, headers=request_headers)

    def delete(self, instance_id: str, idempotency_key: str) -> None:
        """Unregister a webserver process instance"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.delete(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}", headers=request_headers)

    def create_probe(self, instance_id: str, body: Optional[ProbeClusterInstanceRequest] = None) -> ClustersInstancesProbeResponse:
        """Probe one instance's connectivity and record the outcome"""
        return self._client.post(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}/probe", json=body)

    def create_drain(self, instance_id: str) -> ClustersInstancesDrainResponse:
        """Gracefully drain one instance out of routing"""
        return self._client.post(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}/drain")

    def create_undrain(self, instance_id: str) -> ClustersInstancesUndrainResponse:
        """Clear the drain flag and restore routing participation"""
        return self._client.post(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}/undrain")

    def create_cordon(self, instance_id: str) -> ClustersInstancesCordonResponse:
        """Cordon one instance out of routing without draining it"""
        return self._client.post(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}/cordon")

    def create_uncordon(self, instance_id: str) -> ClustersInstancesUncordonResponse:
        """Uncordon one instance back into routing"""
        return self._client.post(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}/uncordon")

class ClusterInstancesHeartbeatsApi:
    """cluster clusters.instances.heartbeats API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def list(self, instance_id: str, page_size: Optional[int] = None, cursor: Optional[str] = None) -> ClustersInstancesHeartbeatsListResponse:
        """List one instance's stored heartbeat samples"""
        query = build_query_string([
            {'name': 'page_size', 'value': page_size, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'cursor', 'value': cursor, 'style': 'form', 'explode': True, 'allow_reserved': False},
        ])
        return self._client.get(_append_query_string(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}/heartbeats", query))

class ClusterInstancesMetricsApi:
    """cluster clusters.instances.metrics API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def list_history(self, instance_id: str, limit: Optional[int] = None) -> ClustersInstancesMetricsListResponse:
        """List one instance's heartbeat metric samples for trend charts"""
        query = build_query_string([
            {'name': 'limit', 'value': limit, 'style': 'form', 'explode': True, 'allow_reserved': False},
        ])
        return self._client.get(_append_query_string(f"/backend/v3/api/clusters/instances/{serialize_path_parameter(instance_id, {'name': 'instanceId', 'style': 'simple', 'explode': False})}/metrics/history", query))

class ClusterEventsApi:
    """cluster clusters.events API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def list(self, page_size: Optional[int] = None, cursor: Optional[str] = None, cluster_id: Optional[str] = None, severity: Optional[str] = None) -> ClustersEventsListResponse:
        """List cluster lifecycle events"""
        query = build_query_string([
            {'name': 'page_size', 'value': page_size, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'cursor', 'value': cursor, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'cluster_id', 'value': cluster_id, 'style': 'form', 'explode': True, 'allow_reserved': False},
            {'name': 'severity', 'value': severity, 'style': 'form', 'explode': True, 'allow_reserved': False},
        ])
        return self._client.get(_append_query_string(f"/backend/v3/api/clusters/events", query))

class ClusterOverviewApi:
    """cluster clusters.overview API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def list(self) -> ClustersOverviewRetrieveResponse:
        """Retrieve the cluster health overview for status polling"""
        return self._client.get(f"/backend/v3/api/clusters/overview")

class ClusterMessagesApi:
    """cluster clusters.messages API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def create(self, body: EnqueueClusterPeerMessagesRequest, idempotency_key: str) -> ClustersMessagesCreateResponse201:
        """Enqueue a peer message to one instance or broadcast to online members"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.post(f"/backend/v3/api/clusters/messages", json=body, headers=request_headers)
