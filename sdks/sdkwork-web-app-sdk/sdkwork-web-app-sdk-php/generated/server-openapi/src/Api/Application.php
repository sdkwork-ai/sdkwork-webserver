<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Api;

use SDKWork\Web\AppSdk\Models\ApplicationsActivateResponse;
use SDKWork\Web\AppSdk\Models\ApplicationsCreateResponse201;
use SDKWork\Web\AppSdk\Models\ApplicationsListResponse;
use SDKWork\Web\AppSdk\Models\ApplicationsPauseResponse;
use SDKWork\Web\AppSdk\Models\ApplicationsRetrieveResponse;
use SDKWork\Web\AppSdk\Models\ApplicationsUpdateResponse;
use SDKWork\Web\AppSdk\Models\CreateApplicationRequest;
use SDKWork\Web\AppSdk\Models\UpdateApplicationRequest;

final class ApplicationApi extends BaseApi
{
    /** 获取应用列表 */
    public function applicationsList(?int $page = null, ?int $pageSize = null, ?int $status = null, ?string $applicationType = null, ?int $siteType = null, ?string $keyword = null): ?ApplicationsListResponse
    {
        $path = '/app/v3/api/applications';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
            new QueryParameterSpec('application_type', $applicationType, 'form', true, false, null),
            new QueryParameterSpec('site_type', $siteType, 'form', true, false, null),
            new QueryParameterSpec('keyword', $keyword, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsListResponse::fromArray($result) : null;
    }

    /** 创建应用 */
    public function applicationsCreate(array|CreateApplicationRequest $body, string $idempotencyKey): ?ApplicationsCreateResponse201
    {
        $path = '/app/v3/api/applications';
        $payload = $body instanceof CreateApplicationRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsCreateResponse201::fromArray($result) : null;
    }

    /** 获取应用详情 */
    public function applicationsRetrieve(string $applicationId): ?ApplicationsRetrieveResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsRetrieveResponse::fromArray($result) : null;
    }

    /** 更新应用 */
    public function applicationsUpdate(string $applicationId, array|UpdateApplicationRequest $body, string $idempotencyKey): ?ApplicationsUpdateResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $payload = $body instanceof UpdateApplicationRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsUpdateResponse::fromArray($result) : null;
    }

    /** 删除应用 */
    public function applicationsDelete(string $applicationId): mixed
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $result = $this->client->request('DELETE', $path, []);
        return $result;
    }

    /** 激活应用 */
    public function applicationsActivate(string $applicationId): ?ApplicationsActivateResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/activate', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $result = $this->client->request('POST', $path, []);
        return is_array($result) ? ApplicationsActivateResponse::fromArray($result) : null;
    }

    /** 暂停应用 */
    public function applicationsPause(string $applicationId): ?ApplicationsPauseResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/pause', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $result = $this->client->request('POST', $path, []);
        return is_array($result) ? ApplicationsPauseResponse::fromArray($result) : null;
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
