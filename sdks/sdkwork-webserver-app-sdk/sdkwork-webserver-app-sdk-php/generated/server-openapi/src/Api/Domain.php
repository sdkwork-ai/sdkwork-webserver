<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Api;

use SDKWork\Web\AppSdk\Models\ApplicationsDomainsCreateResponse201;
use SDKWork\Web\AppSdk\Models\ApplicationsDomainsListResponse;
use SDKWork\Web\AppSdk\Models\ApplicationsDomainsRetrieveResponse;
use SDKWork\Web\AppSdk\Models\ApplicationsDomainsVerifyResponse;
use SDKWork\Web\AppSdk\Models\CreateDomainRequest;
use SDKWork\Web\AppSdk\Models\DomainsListResponse;

final class DomainApi extends BaseApi
{
    /** 获取站点域名列表 */
    public function applicationsDomainsList(string $applicationId, ?int $page = null, ?int $pageSize = null): ?ApplicationsDomainsListResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/domains', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsDomainsListResponse::fromArray($result) : null;
    }

    /** 绑定域名 */
    public function applicationsDomainsCreate(string $applicationId, array|CreateDomainRequest $body, string $idempotencyKey): ?ApplicationsDomainsCreateResponse201
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/domains', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $payload = $body instanceof CreateDomainRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsDomainsCreateResponse201::fromArray($result) : null;
    }

    /** 获取域名详情 */
    public function applicationsDomainsRetrieve(string $applicationId, string $domainId): ?ApplicationsDomainsRetrieveResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/domains/{domainId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsDomainsRetrieveResponse::fromArray($result) : null;
    }

    /** 解绑域名 */
    public function applicationsDomainsDelete(string $applicationId, string $domainId): mixed
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/domains/{domainId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
        $result = $this->client->request('DELETE', $path, []);
        return $result;
    }

    /** 创建或检查域名所有权验证挑战 */
    public function applicationsDomainsVerify(string $applicationId, string $domainId, string $idempotencyKey): ?ApplicationsDomainsVerifyResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/domains/{domainId}/verify', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
        return is_array($result) ? ApplicationsDomainsVerifyResponse::fromArray($result) : null;
    }

    /** 获取证书可签发域名列表 */
    public function domainsList(?int $page = null, ?int $pageSize = null): ?DomainsListResponse
    {
        $path = '/app/v3/api/domains';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? DomainsListResponse::fromArray($result) : null;
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
