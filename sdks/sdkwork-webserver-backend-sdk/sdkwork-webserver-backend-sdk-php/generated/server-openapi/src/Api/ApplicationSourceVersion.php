<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\ApplicationsSourceVersionsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\ApplicationsSourceVersionsGitImportCreateResponse201;
use SDKWork\Web\BackendSdk\Models\ApplicationsSourceVersionsListResponse;
use SDKWork\Web\BackendSdk\Models\ApplicationsSourceVersionsRetrieveResponse;
use SDKWork\Web\BackendSdk\Models\CreateApplicationSourceVersionRequest;
use SDKWork\Web\BackendSdk\Models\ImportApplicationGitSourceVersionRequest;

final class ApplicationSourceVersionApi extends BaseApi
{
    /** List immutable application source versions */
    public function applicationsSourceVersionsList(string $applicationId, ?int $pageSize = null, ?string $cursor = null): ?ApplicationsSourceVersionsListResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/source_versions', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsSourceVersionsListResponse::fromArray($result) : null;
    }

    /** Register an immutable Drive-backed application source version */
    public function applicationsSourceVersionsCreate(string $applicationId, array|CreateApplicationSourceVersionRequest $body, string $idempotencyKey): ?ApplicationsSourceVersionsCreateResponse201
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/source_versions', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $payload = $body instanceof CreateApplicationSourceVersionRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsSourceVersionsCreateResponse201::fromArray($result) : null;
    }

    /** Import an immutable application source version from a public Git repository */
    public function applicationsSourceVersionsGitImportCreate(string $applicationId, array|ImportApplicationGitSourceVersionRequest $body, string $idempotencyKey): ?ApplicationsSourceVersionsGitImportCreateResponse201
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/source_versions/git_import', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $payload = $body instanceof ImportApplicationGitSourceVersionRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsSourceVersionsGitImportCreateResponse201::fromArray($result) : null;
    }

    /** Retrieve an application source version */
    public function applicationsSourceVersionsRetrieve(string $applicationId, string $sourceVersionId): ?ApplicationsSourceVersionsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/source_versions/{sourceVersionId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'sourceVersionId' => $this->serializePathParameter($sourceVersionId, new PathParameterSpec('sourceVersionId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsSourceVersionsRetrieveResponse::fromArray($result) : null;
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
