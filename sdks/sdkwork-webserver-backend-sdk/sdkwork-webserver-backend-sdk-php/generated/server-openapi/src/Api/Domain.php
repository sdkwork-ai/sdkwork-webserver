<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\CreateManagedDomainRequest;
use SDKWork\Web\BackendSdk\Models\CreateRootDomainHostnameRequest;
use SDKWork\Web\BackendSdk\Models\CreateRootDomainRequest;
use SDKWork\Web\BackendSdk\Models\DomainsApplicationBindingUpdateResponse;
use SDKWork\Web\BackendSdk\Models\DomainsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\DomainsListResponse;
use SDKWork\Web\BackendSdk\Models\DomainsVerifyResponse;
use SDKWork\Web\BackendSdk\Models\RootDomainsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\RootDomainsListResponse;
use SDKWork\Web\BackendSdk\Models\RootDomainsRetrieveResponse;
use SDKWork\Web\BackendSdk\Models\RootDomainsSubdomainsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\RootDomainsSubdomainsListResponse;
use SDKWork\Web\BackendSdk\Models\UpdateDomainApplicationBindingRequest;

final class DomainApi extends BaseApi
{
    /** List tenant root-domain Zones */
    public function rootDomainsList(?int $page = null, ?int $pageSize = null, ?int $status = null, ?string $keyword = null): ?RootDomainsListResponse
    {
        $path = '/backend/v3/api/root_domains';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
            new QueryParameterSpec('keyword', $keyword, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? RootDomainsListResponse::fromArray($result) : null;
    }

    /** Define a tenant root-domain Zone */
    public function rootDomainsCreate(array|CreateRootDomainRequest $body, string $idempotencyKey): ?RootDomainsCreateResponse201
    {
        $path = '/backend/v3/api/root_domains';
        $payload = $body instanceof CreateRootDomainRequest ? $body->toArray() : $body;
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
        return is_array($result) ? RootDomainsCreateResponse201::fromArray($result) : null;
    }

    /** Retrieve a tenant root-domain Zone */
    public function rootDomainsRetrieve(string $rootDomainId): ?RootDomainsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/root_domains/{rootDomainId}', ['rootDomainId' => $this->serializePathParameter($rootDomainId, new PathParameterSpec('rootDomainId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? RootDomainsRetrieveResponse::fromArray($result) : null;
    }

    /** Delete an empty tenant root-domain Zone */
    public function rootDomainsDelete(string $rootDomainId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/root_domains/{rootDomainId}', ['rootDomainId' => $this->serializePathParameter($rootDomainId, new PathParameterSpec('rootDomainId', 'simple', false))]);
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

    /** List publishable hostnames in a root-domain Zone */
    public function rootDomainsSubdomainsList(string $rootDomainId, ?int $page = null, ?int $pageSize = null): ?RootDomainsSubdomainsListResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/root_domains/{rootDomainId}/subdomains', ['rootDomainId' => $this->serializePathParameter($rootDomainId, new PathParameterSpec('rootDomainId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? RootDomainsSubdomainsListResponse::fromArray($result) : null;
    }

    /** Add a publishable hostname to a root-domain Zone */
    public function rootDomainsSubdomainsCreate(string $rootDomainId, array|CreateRootDomainHostnameRequest $body, string $idempotencyKey): ?RootDomainsSubdomainsCreateResponse201
    {
        $path = $this->interpolatePath('/backend/v3/api/root_domains/{rootDomainId}/subdomains', ['rootDomainId' => $this->serializePathParameter($rootDomainId, new PathParameterSpec('rootDomainId', 'simple', false))]);
        $payload = $body instanceof CreateRootDomainHostnameRequest ? $body->toArray() : $body;
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
        return is_array($result) ? RootDomainsSubdomainsCreateResponse201::fromArray($result) : null;
    }

    /** List tenant custom domain assets */
    public function domainsList(?int $page = null, ?int $pageSize = null): ?DomainsListResponse
    {
        $path = '/backend/v3/api/domains';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? DomainsListResponse::fromArray($result) : null;
    }

    /** Register a tenant custom domain asset */
    public function domainsCreate(array|CreateManagedDomainRequest $body, string $idempotencyKey): ?DomainsCreateResponse201
    {
        $path = '/backend/v3/api/domains';
        $payload = $body instanceof CreateManagedDomainRequest ? $body->toArray() : $body;
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
        return is_array($result) ? DomainsCreateResponse201::fromArray($result) : null;
    }

    /** Delete an unbound tenant custom domain asset */
    public function domainsDelete(string $domainId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/domains/{domainId}', ['domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
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

    /** Create or check a tenant custom-domain ownership challenge */
    public function domainsVerify(string $domainId, string $idempotencyKey): ?DomainsVerifyResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/domains/{domainId}/verify', ['domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
        return is_array($result) ? DomainsVerifyResponse::fromArray($result) : null;
    }

    /** Bind a tenant custom domain to an application */
    public function domainsApplicationBindingUpdate(string $domainId, array|UpdateDomainApplicationBindingRequest $body, string $idempotencyKey): ?DomainsApplicationBindingUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/domains/{domainId}/application_binding', ['domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
        $payload = $body instanceof UpdateDomainApplicationBindingRequest ? $body->toArray() : $body;
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
        return is_array($result) ? DomainsApplicationBindingUpdateResponse::fromArray($result) : null;
    }

    /** Unbind a tenant custom domain from its application */
    public function domainsApplicationBindingDelete(string $domainId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/domains/{domainId}/application_binding', ['domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
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
