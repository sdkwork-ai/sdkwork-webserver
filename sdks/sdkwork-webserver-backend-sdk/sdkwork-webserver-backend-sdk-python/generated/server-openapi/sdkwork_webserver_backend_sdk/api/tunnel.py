from typing import Any, Dict, List, Optional
from ..http_client import HttpClient
from ..models import CreateTunnelMachineRouteRequest, SetTunnelMachineEnabledRequest, TunnelMachinesEnabledUpdateResponse, TunnelMachinesListResponse, TunnelMachinesRetrieveResponse, TunnelMachinesRoutesCreateResponse201

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


class TunnelApi:
    """tunnel tunnel API client."""

    def __init__(self, client: HttpClient):
        self._client = client
        self.tunnel_machines = TunnelTunnelMachinesApi(client)


class TunnelTunnelMachinesApi:
    """tunnel tunnel.tunnel_machines API client."""

    def __init__(self, client: HttpClient):
        self._client = client
        self.enabled = TunnelTunnelMachinesEnabledApi(client)
        self.routes = TunnelTunnelMachinesRoutesApi(client)


    def list(self) -> TunnelMachinesListResponse:
        """List the tunnel connector machines this gateway manages"""
        return self._client.get(f"/backend/v3/api/tunnel_machines")

    def retrieve(self, device_id: str) -> TunnelMachinesRetrieveResponse:
        """Retrieve one managed tunnel machine"""
        return self._client.get(f"/backend/v3/api/tunnel_machines/{serialize_path_parameter(device_id, {'name': 'deviceId', 'style': 'simple', 'explode': False})}")

class TunnelTunnelMachinesEnabledApi:
    """tunnel tunnel.tunnel_machines.enabled API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def update(self, device_id: str, body: SetTunnelMachineEnabledRequest) -> TunnelMachinesEnabledUpdateResponse:
        """Switch one tunnel machine on or off"""
        return self._client.put(f"/backend/v3/api/tunnel_machines/{serialize_path_parameter(device_id, {'name': 'deviceId', 'style': 'simple', 'explode': False})}/enabled", json=body)

class TunnelTunnelMachinesRoutesApi:
    """tunnel tunnel.tunnel_machines.routes API client."""

    def __init__(self, client: HttpClient):
        self._client = client


    def create(self, device_id: str, body: CreateTunnelMachineRouteRequest, idempotency_key: str) -> TunnelMachinesRoutesCreateResponse201:
        """Declare a route for one tunnel machine"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.post(f"/backend/v3/api/tunnel_machines/{serialize_path_parameter(device_id, {'name': 'deviceId', 'style': 'simple', 'explode': False})}/routes", json=body, headers=request_headers)

    def delete(self, device_id: str, route_id: str, idempotency_key: str) -> None:
        """Remove one route from a tunnel machine"""
        request_headers = build_request_headers(
            {
                'Idempotency-Key': {'value': idempotency_key, 'style': 'simple', 'explode': False},
            },
            {}
        )
        return self._client.delete(f"/backend/v3/api/tunnel_machines/{serialize_path_parameter(device_id, {'name': 'deviceId', 'style': 'simple', 'explode': False})}/routes/{serialize_path_parameter(route_id, {'name': 'routeId', 'style': 'simple', 'explode': False})}", headers=request_headers)
