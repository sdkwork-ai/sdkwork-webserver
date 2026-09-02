import json as json_module
import requests

from sdkwork.common.core.types import SdkConfig as CommonSdkConfig
from sdkwork.common.http import BaseHttpClient

SdkConfig = CommonSdkConfig


class HttpClient(BaseHttpClient):
    """
    SDK HTTP client wrapper based on sdkwork-common.

    Auth headers:
    - auth_token -> Authorization: Bearer {auth_token}
    - access_token -> Access-Token: {access_token}
    """

    def _update_auth_headers(self) -> None:
        if self._session is None:
            return

        self._session.headers.pop('Authorization', None)
        self._session.headers.pop('Access-Token', None)
        self._session.headers.pop('X-API-Key', None)
        if self._auth_token:
            self._session.headers['Authorization'] = f'Bearer {self._auth_token}'
        if self._access_token:
            self._session.headers['Access-Token'] = self._access_token
    def set_auth_token(self, token: str) -> 'HttpClient':
        self._auth_token = token
        self._update_auth_headers()
        return self
    def set_access_token(self, token: str) -> 'HttpClient':
        self._access_token = token
        self._update_auth_headers()
        return self

    def set_header(self, key: str, value: str) -> 'HttpClient':
        self.headers[key] = value
        if self._session is not None:
            self._session.headers[key] = value
        return self

    @staticmethod
    def _strip_credential_headers(headers):
        forbidden = {
            'authorization', 'access-token', 'x-api-key', 'x-tenant-id',
            'x-organization-id', 'x-platform', 'x-user-id',
            'x-sdkwork-tenant-id', 'x-sdkwork-organization-id', 'x-sdkwork-user-id',
        }
        return {key: value for key, value in (headers or {}).items() if key.lower() not in forbidden}

    def _request_headers(self, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        if skip_auth or access_token_only:
            request_headers = self._strip_credential_headers(headers)
            if access_token_only:
                access_token = (self._access_token or '').strip()
                if not access_token:
                    raise RuntimeError('access-token-only request requires Access-Token before request dispatch')
                request_headers['Access-Token'] = access_token
            return request_headers
        return headers

    def _request_session(self, skip_auth: bool = False, access_token_only: bool = False):
        if not skip_auth and not access_token_only:
            return self._get_session()
        session = requests.Session()
        session.headers.clear()
        return session

    def request(self, method: str, path: str, params=None, data=None, json=None, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        response = self._request_session(skip_auth, access_token_only).request(
            method=method,
            url=f"{self.base_url}{path}",
            params=params,
            data=data,
            json=json,
            headers=self._request_headers(headers, skip_auth, access_token_only),
            timeout=self.timeout / 1000,
        )
        response.raise_for_status()
        if not response.content:
            return None
        return response.json()

    def request_bytes(self, method: str, path: str, params=None, data=None, json=None, headers=None, skip_auth: bool = False, access_token_only: bool = False) -> bytes:
        response = self._request_session(skip_auth, access_token_only).request(
            method=method,
            url=f"{self.base_url}{path}",
            params=params,
            data=data,
            json=json,
            headers=self._request_headers(headers, skip_auth, access_token_only),
            timeout=self.timeout / 1000,
        )
        response.raise_for_status()
        return response.content

    def get(self, path: str, params=None, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        return self.request('GET', path, params=params, headers=headers, skip_auth=skip_auth, access_token_only=access_token_only)

    def post(self, path: str, params=None, data=None, json=None, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        return self.request('POST', path, params=params, data=data, json=json, headers=headers, skip_auth=skip_auth, access_token_only=access_token_only)

    def put(self, path: str, params=None, data=None, json=None, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        return self.request('PUT', path, params=params, data=data, json=json, headers=headers, skip_auth=skip_auth, access_token_only=access_token_only)

    def patch(self, path: str, params=None, data=None, json=None, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        return self.request('PATCH', path, params=params, data=data, json=json, headers=headers, skip_auth=skip_auth, access_token_only=access_token_only)

    def delete(self, path: str, params=None, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        return self.request('DELETE', path, params=params, headers=headers, skip_auth=skip_auth, access_token_only=access_token_only)

    def stream_json(self, path: str, method: str = 'POST', params=None, data=None, json=None, headers=None, skip_auth: bool = False, access_token_only: bool = False):
        response = self._request_session(skip_auth, access_token_only).request(
            method=method,
            url=f"{self.base_url}{path}",
            params=params,
            data=data,
            json=json,
            headers=self._request_headers({'Accept': 'text/event-stream', **(headers or {})}, skip_auth, access_token_only),
            timeout=self.timeout / 1000,
            stream=True,
        )
        response.raise_for_status()
        for line in response.iter_lines(decode_unicode=True):
            if not line or line.startswith(':'):
                continue
            if not line.startswith('data:'):
                continue
            data = line[5:].strip()
            if data == '[DONE]':
                break
            yield json_module.loads(data)
