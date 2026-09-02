<?php

declare(strict_types=1);

namespace SDKWork\Webserver\AppSdk\Api;

use SDKWork\Webserver\AppSdk\Models\ApplicationsEnvVariablesCreateResponse201;
use SDKWork\Webserver\AppSdk\Models\ApplicationsEnvVariablesListResponse;
use SDKWork\Webserver\AppSdk\Models\ApplicationsEnvVariablesUpdateResponse;
use SDKWork\Webserver\AppSdk\Models\CreateEnvVariableRequest;
use SDKWork\Webserver\AppSdk\Models\UpdateEnvVariableRequest;

final class EnvVariableApi extends BaseApi
{
    /** 获取环境变量列表 */
    public function applicationsEnvVariablesList(string $applicationId, ?string $environment = null): ?ApplicationsEnvVariablesListResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/env_variables', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('environment', $environment, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsEnvVariablesListResponse::fromArray($result) : null;
    }

    /** 创建环境变量 */
    public function applicationsEnvVariablesCreate(string $applicationId, array|CreateEnvVariableRequest $body, string $idempotencyKey): ?ApplicationsEnvVariablesCreateResponse201
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/env_variables', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $payload = $body instanceof CreateEnvVariableRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsEnvVariablesCreateResponse201::fromArray($result) : null;
    }

    /** 轮换环境变量值 */
    public function applicationsEnvVariablesUpdate(string $applicationId, string $variableId, array|UpdateEnvVariableRequest $body, string $idempotencyKey): ?ApplicationsEnvVariablesUpdateResponse
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/env_variables/{variableId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'variableId' => $this->serializePathParameter($variableId, new PathParameterSpec('variableId', 'simple', false))]);
        $payload = $body instanceof UpdateEnvVariableRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ApplicationsEnvVariablesUpdateResponse::fromArray($result) : null;
    }

    /** 删除环境变量 */
    public function applicationsEnvVariablesDelete(string $applicationId, string $variableId, string $idempotencyKey): void
    {
        $path = $this->interpolatePath('/app/v3/api/applications/{applicationId}/env_variables/{variableId}', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false)), 'variableId' => $this->serializePathParameter($variableId, new PathParameterSpec('variableId', 'simple', false))]);
        $requestHeaders = $this->buildRequestHeaders(
            [
                'Idempotency-Key' => new HeaderParameterSpec($idempotencyKey, 'simple', false, null),
            ],
            []
        );
        $this->client->request('DELETE', $path, [
            'headers' => $requestHeaders,
        ]);
        return;
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
