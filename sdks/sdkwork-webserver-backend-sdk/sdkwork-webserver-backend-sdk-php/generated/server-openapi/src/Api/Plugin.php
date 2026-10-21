<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Api;

use SDKWork\Webserver\BackendSdk\Models\CreatePluginRequest;
use SDKWork\Webserver\BackendSdk\Models\PluginsCreateResponse201;
use SDKWork\Webserver\BackendSdk\Models\PluginsListResponse;
use SDKWork\Webserver\BackendSdk\Models\PluginsRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\PluginsUpdateResponse;
use SDKWork\Webserver\BackendSdk\Models\UpdatePluginRequest;

final class PluginApi extends BaseApi
{
    /** List the caller's plugin catalog */
    public function pluginsList(?int $page = null, ?int $pageSize = null, ?string $status = null, ?string $q = null, ?string $ownerId = null): ?PluginsListResponse
    {
        $path = '/backend/v3/api/plugins';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
            new QueryParameterSpec('q', $q, 'form', true, false, null),
            new QueryParameterSpec('owner_id', $ownerId, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? PluginsListResponse::fromArray($result) : null;
    }

    /** Create a plugin catalog record */
    public function pluginsCreate(array|CreatePluginRequest $body, string $idempotencyKey): ?PluginsCreateResponse201
    {
        $path = '/backend/v3/api/plugins';
        $payload = $body instanceof CreatePluginRequest ? $body->toArray() : $body;
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
            'json' => $payload,
        ]);
        return is_array($result) ? PluginsCreateResponse201::fromArray($result) : null;
    }

    /** Retrieve one plugin catalog record */
    public function pluginsRetrieve(string $pluginId): ?PluginsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/plugins/{pluginId}', ['pluginId' => $this->serializePathParameter($pluginId, new PathParameterSpec('pluginId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? PluginsRetrieveResponse::fromArray($result) : null;
    }

    /** Update one plugin catalog record */
    public function pluginsUpdate(string $pluginId, array|UpdatePluginRequest $body, string $idempotencyKey): ?PluginsUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/plugins/{pluginId}', ['pluginId' => $this->serializePathParameter($pluginId, new PathParameterSpec('pluginId', 'simple', false))]);
        $payload = $body instanceof UpdatePluginRequest ? $body->toArray() : $body;
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('PATCH', $path, [
            'headers' => $requestHeaders,
            'json' => $payload,
        ]);
        return is_array($result) ? PluginsUpdateResponse::fromArray($result) : null;
    }

    /** Delete one plugin catalog record */
    public function pluginsDelete(string $pluginId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/plugins/{pluginId}', ['pluginId' => $this->serializePathParameter($pluginId, new PathParameterSpec('pluginId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('DELETE', $path, [
            'headers' => $requestHeaders,
        ]);
        return $result;
    }

    private function buildRequestHeaders(array $headers, array $cookies): array
    {
        $requestHeaders = [];
        foreach ($headers as $name => $parameter) {
            $serialized = $this->serializeParameterValue($parameter);
            if ($serialized !== null) {
                $requestHeaders[(string) $name] = $serialized;
            }
        }

        $cookieHeader = $this->buildCookieHeader($cookies);
        if ($cookieHeader !== '') {
            $requestHeaders['Cookie'] = isset($requestHeaders['Cookie']) && $requestHeaders['Cookie'] !== ''
                ? $requestHeaders['Cookie'] . '; ' . $cookieHeader
                : $cookieHeader;
        }

        return $requestHeaders;
    }

    private function buildCookieHeader(array $cookies): string
    {
        $pairs = [];
        foreach ($cookies as $name => $parameter) {
            $serialized = $this->serializeParameterValue($parameter);
            if ($serialized !== null) {
                $pairs[] = rawurlencode((string) $name) . '=' . rawurlencode($serialized);
            }
        }

        return implode('; ', $pairs);
    }

    private function serializeParameterValue(?HeaderParameterSpec $parameter): ?string
    {
        $value = $parameter?->value;
        if ($value === null) {
            return null;
        }
        if ($parameter->contentType !== null && trim($parameter->contentType) !== '') {
            return (string) json_encode($value, JSON_UNESCAPED_SLASHES);
        }
        if (is_array($value)) {
            $serialized = [];
            foreach ($value as $key => $item) {
                if ($item === null) {
                    continue;
                }
                if (!array_is_list($value) && $parameter->explode) {
                    $serialized[] = (string) $key . '=' . (string) $item;
                } elseif (!array_is_list($value)) {
                    $serialized[] = (string) $key;
                    $serialized[] = (string) $item;
                } else {
                    $serialized[] = (string) $item;
                }
            }
            return implode(',', $serialized);
        }
        if ($value instanceof \Stringable) {
            return (string) $value;
        }

        return (string) $value;
    }
}
