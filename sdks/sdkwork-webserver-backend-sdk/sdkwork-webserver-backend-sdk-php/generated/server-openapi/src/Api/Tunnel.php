<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Api;

use SDKWork\Webserver\BackendSdk\Models\CreateTunnelMachineRouteRequest;
use SDKWork\Webserver\BackendSdk\Models\SetTunnelMachineEnabledRequest;
use SDKWork\Webserver\BackendSdk\Models\TunnelMachinesEnabledUpdateResponse;
use SDKWork\Webserver\BackendSdk\Models\TunnelMachinesListResponse;
use SDKWork\Webserver\BackendSdk\Models\TunnelMachinesRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\TunnelMachinesRoutesCreateResponse201;

final class TunnelApi extends BaseApi
{
    /** List the tunnel connector machines this gateway manages */
    public function machinesList(): ?TunnelMachinesListResponse
    {
        $path = '/backend/v3/api/tunnel_machines';
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? TunnelMachinesListResponse::fromArray($result) : null;
    }

    /** Retrieve one managed tunnel machine */
    public function machinesRetrieve(string $deviceId): ?TunnelMachinesRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/tunnel_machines/{deviceId}', ['deviceId' => $this->serializePathParameter($deviceId, new PathParameterSpec('deviceId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? TunnelMachinesRetrieveResponse::fromArray($result) : null;
    }

    /** Switch one tunnel machine on or off */
    public function machinesEnabledUpdate(string $deviceId, array|SetTunnelMachineEnabledRequest $body): ?TunnelMachinesEnabledUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/tunnel_machines/{deviceId}/enabled', ['deviceId' => $this->serializePathParameter($deviceId, new PathParameterSpec('deviceId', 'simple', false))]);
        $payload = $body instanceof SetTunnelMachineEnabledRequest ? $body->toArray() : $body;
        $result = $this->client->request('PUT', $path, [
            'json' => $payload,
        ]);
        return is_array($result) ? TunnelMachinesEnabledUpdateResponse::fromArray($result) : null;
    }

    /** Declare a route for one tunnel machine */
    public function machinesRoutesCreate(string $deviceId, array|CreateTunnelMachineRouteRequest $body, string $idempotencyKey): ?TunnelMachinesRoutesCreateResponse201
    {
        $path = $this->interpolatePath('/backend/v3/api/tunnel_machines/{deviceId}/routes', ['deviceId' => $this->serializePathParameter($deviceId, new PathParameterSpec('deviceId', 'simple', false))]);
        $payload = $body instanceof CreateTunnelMachineRouteRequest ? $body->toArray() : $body;
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
        return is_array($result) ? TunnelMachinesRoutesCreateResponse201::fromArray($result) : null;
    }

    /** Remove one route from a tunnel machine */
    public function machinesRoutesDelete(string $deviceId, string $routeId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/tunnel_machines/{deviceId}/routes/{routeId}', ['deviceId' => $this->serializePathParameter($deviceId, new PathParameterSpec('deviceId', 'simple', false)), 'routeId' => $this->serializePathParameter($routeId, new PathParameterSpec('routeId', 'simple', false))]);
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
