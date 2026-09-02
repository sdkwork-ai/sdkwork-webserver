<?php

declare(strict_types=1);

namespace SDKWork\Webserver\AppSdk\Api;

use SDKWork\Webserver\AppSdk\Models\ApplicationsDeploymentsCreateResponse201;
use SDKWork\Webserver\AppSdk\Models\ApplicationsDeploymentsListResponse;
use SDKWork\Webserver\AppSdk\Models\ApplicationsDeploymentsRetrieveResponse;
use SDKWork\Webserver\AppSdk\Models\ApplicationsDeploymentsRollbackResponse;
use SDKWork\Webserver\AppSdk\Models\CreateDeploymentRequest;

final class DeploymentApi extends BaseApi
{
    /** 获取部署历史 */
    public function applicationsDeploymentsList(string $applicationId, ?int $pageSize = null, ?string $cursor = null, ?int $status = null): ?ApplicationsDeploymentsListResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/deployments', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsDeploymentsListResponse::fromArray($result) : null;
    }

    /** 发起部署 */
    public function applicationsDeploymentsCreate(string $applicationId, array|CreateDeploymentRequest $body, string $idempotencyKey): ?ApplicationsDeploymentsCreateResponse201
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/deployments', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $payload = $body instanceof CreateDeploymentRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsDeploymentsCreateResponse201::fromArray($result) : null;
    }

    /** 获取部署详情 */
    public function applicationsDeploymentsRetrieve(string $applicationId, string $deploymentId): ?ApplicationsDeploymentsRetrieveResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/deployments/{deploymentId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'deploymentId' => $this->serializePathParameter($deploymentId, new PathParameterSpec('deploymentId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsDeploymentsRetrieveResponse::fromArray($result) : null;
    }

    /** 基于历史成功版本创建快速还原命令 */
    public function applicationsDeploymentsRollback(string $applicationId, string $deploymentId, string $idempotencyKey): ?ApplicationsDeploymentsRollbackResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/deployments/{deploymentId}/rollback', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'deploymentId' => $this->serializePathParameter($deploymentId, new PathParameterSpec('deploymentId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $result = $this->client->request('POST', $path, [
            'headers' => $requestHeaders,
        ]);
        return is_array($result) ? ApplicationsDeploymentsRollbackResponse::fromArray($result) : null;
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
