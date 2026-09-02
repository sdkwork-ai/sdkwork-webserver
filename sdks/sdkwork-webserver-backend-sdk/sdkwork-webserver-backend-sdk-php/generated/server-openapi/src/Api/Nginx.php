<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\ConfigsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\ConfigsDeployResponse;
use SDKWork\Web\BackendSdk\Models\ConfigsListResponse;
use SDKWork\Web\BackendSdk\Models\ConfigsRetrieveResponse;
use SDKWork\Web\BackendSdk\Models\ConfigsUpdateResponse;
use SDKWork\Web\BackendSdk\Models\ConfigsValidateResponse;
use SDKWork\Web\BackendSdk\Models\CreateNginxConfigRequest;
use SDKWork\Web\BackendSdk\Models\ReloadResponse;
use SDKWork\Web\BackendSdk\Models\StatusRetrieveResponse;
use SDKWork\Web\BackendSdk\Models\UpdateNginxConfigRequest;

final class NginxApi extends BaseApi
{
    /** List Nginx configurations */
    public function configsList(?int $page = null, ?int $pageSize = null, ?string $siteId = null, ?int $configType = null, ?bool $isActive = null): ?ConfigsListResponse
    {
        $path = '/backend/v3/api/nginx/configs';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('site_id', $siteId, 'form', true, false, null),
            new QueryParameterSpec('config_type', $configType, 'form', true, false, null),
            new QueryParameterSpec('is_active', $isActive, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ConfigsListResponse::fromArray($result) : null;
    }

    /** Create an Nginx configuration */
    public function configsCreate(array|CreateNginxConfigRequest $body, string $idempotencyKey): ?ConfigsCreateResponse201
    {
        $path = '/backend/v3/api/nginx/configs';
        $payload = $body instanceof CreateNginxConfigRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ConfigsCreateResponse201::fromArray($result) : null;
    }

    /** Retrieve an Nginx configuration */
    public function configsRetrieve(string $configId): ?ConfigsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/nginx/etc/{configId}', ['configId' => $this->serializePathParameter($configId, new PathParameterSpec('configId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ConfigsRetrieveResponse::fromArray($result) : null;
    }

    /** Update an Nginx configuration */
    public function configsUpdate(string $configId, array|UpdateNginxConfigRequest $body, string $idempotencyKey): ?ConfigsUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/nginx/etc/{configId}', ['configId' => $this->serializePathParameter($configId, new PathParameterSpec('configId', 'simple', false))]);
        $payload = $body instanceof UpdateNginxConfigRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ConfigsUpdateResponse::fromArray($result) : null;
    }

    /** Validate an Nginx configuration */
    public function configsValidate(string $configId): ?ConfigsValidateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/nginx/etc/{configId}/validate', ['configId' => $this->serializePathParameter($configId, new PathParameterSpec('configId', 'simple', false))]);
        $result = $this->client->request('POST', $path, []);
        return is_array($result) ? ConfigsValidateResponse::fromArray($result) : null;
    }

    /** Deploy an Nginx configuration */
    public function configsDeploy(string $configId, string $idempotencyKey): ?ConfigsDeployResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/nginx/etc/{configId}/deploy', ['configId' => $this->serializePathParameter($configId, new PathParameterSpec('configId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
        return is_array($result) ? ConfigsDeployResponse::fromArray($result) : null;
    }

    /** Reload Nginx */
    public function reload(string $idempotencyKey): ?ReloadResponse
    {
        $path = '/backend/v3/api/nginx/reload';
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
        return is_array($result) ? ReloadResponse::fromArray($result) : null;
    }

    /** Retrieve Nginx status */
    public function statusRetrieve(): ?StatusRetrieveResponse
    {
        $path = '/backend/v3/api/nginx/status';
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? StatusRetrieveResponse::fromArray($result) : null;
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
