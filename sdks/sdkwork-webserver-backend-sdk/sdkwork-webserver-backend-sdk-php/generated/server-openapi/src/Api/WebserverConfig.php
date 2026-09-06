<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Api;

use SDKWork\Webserver\BackendSdk\Models\WebserverConfigsListResponse;
use SDKWork\Webserver\BackendSdk\Models\WebserverConfigsRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\WebserverConfigsUpdateResponse;
use SDKWork\Webserver\BackendSdk\Models\WebserverConfigWriteRequest;

final class WebserverConfigApi extends BaseApi
{
    /** List the managed Web Server configuration catalog */
    public function webserverConfigsList(): ?WebserverConfigsListResponse
    {
        $path = '/backend/v3/api/webserver_configs';
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? WebserverConfigsListResponse::fromArray($result) : null;
    }

    /** Read one managed configuration file */
    public function webserverConfigsRetrieve(string $configId): ?WebserverConfigsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/webserver_configs/{configId}', ['configId' => $this->serializePathParameter($configId, new PathParameterSpec('configId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? WebserverConfigsRetrieveResponse::fromArray($result) : null;
    }

    /** Validate and atomically overwrite one managed configuration file */
    public function webserverConfigsUpdate(string $configId, array|WebserverConfigWriteRequest $body, string $idempotencyKey): ?WebserverConfigsUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/webserver_configs/{configId}', ['configId' => $this->serializePathParameter($configId, new PathParameterSpec('configId', 'simple', false))]);
        $payload = $body instanceof WebserverConfigWriteRequest ? $body->toArray() : $body;
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('PUT', $path, [
            'headers' => $requestHeaders,
            'json' => $payload,
        ]);
        return is_array($result) ? WebserverConfigsUpdateResponse::fromArray($result) : null;
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
