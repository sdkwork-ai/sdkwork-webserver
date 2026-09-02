<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\ApplicationsDomainsListenerCertificateBindingsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\ApplicationsDomainsListenerCertificateBindingsListResponse;
use SDKWork\Web\BackendSdk\Models\CertificatesIssueResponse202;
use SDKWork\Web\BackendSdk\Models\CertificatesListResponse;
use SDKWork\Web\BackendSdk\Models\CertificatesOperationsRetrieveResponse;
use SDKWork\Web\BackendSdk\Models\CertificatesRenewResponse202;
use SDKWork\Web\BackendSdk\Models\CertificatesRevokeResponse;
use SDKWork\Web\BackendSdk\Models\CertificatesUpdateResponse;
use SDKWork\Web\BackendSdk\Models\CreateListenerCertificateBindingRequest;
use SDKWork\Web\BackendSdk\Models\IssueCertificateRequest;
use SDKWork\Web\BackendSdk\Models\RevokeCertificateRequest;
use SDKWork\Web\BackendSdk\Models\UpdateCertificateRequest;

final class CertificateApi extends BaseApi
{
    /** List certificates active on an application domain listener */
    public function applicationsDomainsListenerCertificateBindingsList(string $applicationId, string $domainId, ?int $page = null, ?int $pageSize = null): ?ApplicationsDomainsListenerCertificateBindingsListResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsDomainsListenerCertificateBindingsListResponse::fromArray($result) : null;
    }

    /** Bind a certificate version to an application domain listener */
    public function applicationsDomainsListenerCertificateBindingsCreate(string $applicationId, string $domainId, array|CreateListenerCertificateBindingRequest $body, string $idempotencyKey): ?ApplicationsDomainsListenerCertificateBindingsCreateResponse201
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false))]);
        $payload = $body instanceof CreateListenerCertificateBindingRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsDomainsListenerCertificateBindingsCreateResponse201::fromArray($result) : null;
    }

    /** Remove a certificate from an application domain listener */
    public function applicationsDomainsListenerCertificateBindingsDelete(string $applicationId, string $domainId, string $bindingId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings/{bindingId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'domainId' => $this->serializePathParameter($domainId, new PathParameterSpec('domainId', 'simple', false)), 'bindingId' => $this->serializePathParameter($bindingId, new PathParameterSpec('bindingId', 'simple', false))]);
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

    /** List canonical certificates */
    public function certificatesList(?int $page = null, ?int $pageSize = null, ?string $domainId = null): ?CertificatesListResponse
    {
        $path = '/backend/v3/api/certificates';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('domain_id', $domainId, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? CertificatesListResponse::fromArray($result) : null;
    }

    /** Issue a canonical certificate */
    public function certificatesIssue(array|IssueCertificateRequest $body, string $idempotencyKey): ?CertificatesIssueResponse202
    {
        $path = '/backend/v3/api/certificates/issue';
        $payload = $body instanceof IssueCertificateRequest ? $body->toArray() : $body;
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
        return is_array($result) ? CertificatesIssueResponse202::fromArray($result) : null;
    }

    /** Retrieve a certificate operation */
    public function certificatesOperationsRetrieve(string $operationId): ?CertificatesOperationsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/certificates/operations/{operationId}', ['operationId' => $this->serializePathParameter($operationId, new PathParameterSpec('operationId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? CertificatesOperationsRetrieveResponse::fromArray($result) : null;
    }

    /** Update certificate automatic renewal policy */
    public function certificatesUpdate(string $certificateId, array|UpdateCertificateRequest $body, string $idempotencyKey): ?CertificatesUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/certificates/{certificateId}', ['certificateId' => $this->serializePathParameter($certificateId, new PathParameterSpec('certificateId', 'simple', false))]);
        $payload = $body instanceof UpdateCertificateRequest ? $body->toArray() : $body;
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
        return is_array($result) ? CertificatesUpdateResponse::fromArray($result) : null;
    }

    /** Soft-delete a certificate and release its domain identifiers */
    public function certificatesDelete(string $certificateId, string $idempotencyKey): mixed
    {
        $path = $this->interpolatePath('/backend/v3/api/certificates/{certificateId}', ['certificateId' => $this->serializePathParameter($certificateId, new PathParameterSpec('certificateId', 'simple', false))]);
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

    /** Renew a canonical certificate now */
    public function certificatesRenew(string $certificateId, string $idempotencyKey): ?CertificatesRenewResponse202
    {
        $path = $this->interpolatePath('/backend/v3/api/certificates/{certificateId}/renew', ['certificateId' => $this->serializePathParameter($certificateId, new PathParameterSpec('certificateId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
        return is_array($result) ? CertificatesRenewResponse202::fromArray($result) : null;
    }

    /** Revoke a canonical certificate */
    public function certificatesRevoke(string $certificateId, array|RevokeCertificateRequest $body, string $idempotencyKey): ?CertificatesRevokeResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/certificates/{certificateId}/revoke', ['certificateId' => $this->serializePathParameter($certificateId, new PathParameterSpec('certificateId', 'simple', false))]);
        $payload = $body instanceof RevokeCertificateRequest ? $body->toArray() : $body;
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
        return is_array($result) ? CertificatesRevokeResponse::fromArray($result) : null;
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
