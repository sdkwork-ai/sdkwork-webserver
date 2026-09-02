<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\ApplicationsActivateResponse;
use SDKWork\Web\BackendSdk\Models\ApplicationsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\ApplicationsListResponse;
use SDKWork\Web\BackendSdk\Models\ApplicationsPauseResponse;
use SDKWork\Web\BackendSdk\Models\ApplicationsRetrieveResponse;
use SDKWork\Web\BackendSdk\Models\ApplicationsUpdateResponse;
use SDKWork\Web\BackendSdk\Models\CreateApplicationRequest;
use SDKWork\Web\BackendSdk\Models\UpdateApplicationRequest;

final class ApplicationApi extends BaseApi
{
    /** List managed applications */
    public function applicationsList(?int $page = null, ?int $pageSize = null, ?string $applicationType = null, ?int $siteType = null, ?int $status = null, ?string $keyword = null): ?ApplicationsListResponse
    {
        $path = '/backend/v3/api/applications';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('application_type', $applicationType, 'form', true, false, null),
            new QueryParameterSpec('site_type', $siteType, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
            new QueryParameterSpec('keyword', $keyword, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsListResponse::fromArray($result) : null;
    }

    /** Create a managed application */
    public function applicationsCreate(array|CreateApplicationRequest $body, string $idempotencyKey): ?ApplicationsCreateResponse201
    {
        $path = '/backend/v3/api/applications';
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

    /** Retrieve a managed application */
    public function applicationsRetrieve(string $applicationId): ?ApplicationsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsRetrieveResponse::fromArray($result) : null;
    }

    /** Update a managed application */
    public function applicationsUpdate(string $applicationId, array|UpdateApplicationRequest $body, string $idempotencyKey): ?ApplicationsUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
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

    /** Delete a managed application */
    public function applicationsDelete(string $applicationId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
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

    /** Activate a managed application */
    public function applicationsActivate(string $applicationId, string $idempotencyKey): ?ApplicationsActivateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/activate', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
        return is_array($result) ? ApplicationsActivateResponse::fromArray($result) : null;
    }

    /** Pause a managed application */
    public function applicationsPause(string $applicationId, string $idempotencyKey): ?ApplicationsPauseResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/pause', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
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
