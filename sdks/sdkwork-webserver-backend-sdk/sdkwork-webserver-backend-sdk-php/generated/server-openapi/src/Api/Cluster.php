<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Api;

use SDKWork\Webserver\BackendSdk\Models\ClustersCreateResponse201;
use SDKWork\Webserver\BackendSdk\Models\ClustersEventsListResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersHostsListResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersHostsRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersHostsUpdateResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersInstancesHeartbeatsListResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersInstancesListResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersInstancesRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersInstancesUpdateResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersListResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersMessagesCreateResponse201;
use SDKWork\Webserver\BackendSdk\Models\ClustersOverviewRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\ClustersUpdateResponse;
use SDKWork\Webserver\BackendSdk\Models\CreateClusterRequest;
use SDKWork\Webserver\BackendSdk\Models\EnqueueClusterPeerMessagesRequest;
use SDKWork\Webserver\BackendSdk\Models\UpdateClusterHostRequest;
use SDKWork\Webserver\BackendSdk\Models\UpdateClusterInstanceRequest;
use SDKWork\Webserver\BackendSdk\Models\UpdateClusterRequest;

final class ClusterApi extends BaseApi
{
    /** List Web Server clusters */
    public function clustersList(?int $page = null, ?int $pageSize = null): ?ClustersListResponse
    {
        $path = '/backend/v3/api/clusters';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersListResponse::fromArray($result) : null;
    }

    /** Create a Web Server cluster */
    public function clustersCreate(array|CreateClusterRequest $body, string $idempotencyKey): ?ClustersCreateResponse201
    {
        $path = '/backend/v3/api/clusters';
        $payload = $body instanceof CreateClusterRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ClustersCreateResponse201::fromArray($result) : null;
    }

    /** Retrieve a Web Server cluster */
    public function clustersRetrieve(string $clusterId): ?ClustersRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/{clusterId}', ['clusterId' => $this->serializePathParameter($clusterId, new PathParameterSpec('clusterId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersRetrieveResponse::fromArray($result) : null;
    }

    /** Update a Web Server cluster */
    public function clustersUpdate(string $clusterId, array|UpdateClusterRequest $body, string $idempotencyKey): ?ClustersUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/{clusterId}', ['clusterId' => $this->serializePathParameter($clusterId, new PathParameterSpec('clusterId', 'simple', false))]);
        $payload = $body instanceof UpdateClusterRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ClustersUpdateResponse::fromArray($result) : null;
    }

    /** Delete an empty Web Server cluster */
    public function clustersDelete(string $clusterId, string $idempotencyKey): void
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/{clusterId}', ['clusterId' => $this->serializePathParameter($clusterId, new PathParameterSpec('clusterId', 'simple', false))]);
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

    /** List cluster hosts with system and network identity */
    public function clustersHostsList(?int $pageSize = null, ?string $cursor = null, ?string $clusterId = null, ?int $status = null): ?ClustersHostsListResponse
    {
        $path = '/backend/v3/api/clusters/hosts';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
            new QueryParameterSpec('cluster_id', $clusterId, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersHostsListResponse::fromArray($result) : null;
    }

    /** Retrieve a cluster host */
    public function clustersHostsRetrieve(string $hostId): ?ClustersHostsRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/hosts/{hostId}', ['hostId' => $this->serializePathParameter($hostId, new PathParameterSpec('hostId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersHostsRetrieveResponse::fromArray($result) : null;
    }

    /** Rename a host or reassign it to another cluster */
    public function clustersHostsUpdate(string $hostId, array|UpdateClusterHostRequest $body, string $idempotencyKey): ?ClustersHostsUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/hosts/{hostId}', ['hostId' => $this->serializePathParameter($hostId, new PathParameterSpec('hostId', 'simple', false))]);
        $payload = $body instanceof UpdateClusterHostRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ClustersHostsUpdateResponse::fromArray($result) : null;
    }

    /** Remove an instance-free host from the cluster inventory */
    public function clustersHostsDelete(string $hostId, string $idempotencyKey): void
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/hosts/{hostId}', ['hostId' => $this->serializePathParameter($hostId, new PathParameterSpec('hostId', 'simple', false))]);
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

    /** List webserver process instances with liveness state */
    public function clustersInstancesList(?int $pageSize = null, ?string $cursor = null, ?string $clusterId = null, ?string $hostId = null, ?int $status = null, ?string $healthState = null): ?ClustersInstancesListResponse
    {
        $path = '/backend/v3/api/clusters/instances';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
            new QueryParameterSpec('cluster_id', $clusterId, 'form', true, false, null),
            new QueryParameterSpec('host_id', $hostId, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
            new QueryParameterSpec('health_state', $healthState, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersInstancesListResponse::fromArray($result) : null;
    }

    /** Retrieve a webserver process instance */
    public function clustersInstancesRetrieve(string $instanceId): ?ClustersInstancesRetrieveResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/instances/{instanceId}', ['instanceId' => $this->serializePathParameter($instanceId, new PathParameterSpec('instanceId', 'simple', false))]);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersInstancesRetrieveResponse::fromArray($result) : null;
    }

    /** Update an instance display name, status, or advertised endpoint */
    public function clustersInstancesUpdate(string $instanceId, array|UpdateClusterInstanceRequest $body, string $idempotencyKey): ?ClustersInstancesUpdateResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/instances/{instanceId}', ['instanceId' => $this->serializePathParameter($instanceId, new PathParameterSpec('instanceId', 'simple', false))]);
        $payload = $body instanceof UpdateClusterInstanceRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ClustersInstancesUpdateResponse::fromArray($result) : null;
    }

    /** Unregister a webserver process instance */
    public function clustersInstancesDelete(string $instanceId, string $idempotencyKey): void
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/instances/{instanceId}', ['instanceId' => $this->serializePathParameter($instanceId, new PathParameterSpec('instanceId', 'simple', false))]);
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

    /** List cluster lifecycle events */
    public function clustersEventsList(?int $pageSize = null, ?string $cursor = null, ?string $clusterId = null, ?string $severity = null): ?ClustersEventsListResponse
    {
        $path = '/backend/v3/api/clusters/events';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
            new QueryParameterSpec('cluster_id', $clusterId, 'form', true, false, null),
            new QueryParameterSpec('severity', $severity, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersEventsListResponse::fromArray($result) : null;
    }

    /** Retrieve the cluster health overview for status polling */
    public function clustersOverviewRetrieve(): ?ClustersOverviewRetrieveResponse
    {
        $path = '/backend/v3/api/clusters/overview';
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersOverviewRetrieveResponse::fromArray($result) : null;
    }

    /** List one instance's stored heartbeat samples */
    public function clustersInstancesHeartbeatsList(string $instanceId, ?int $pageSize = null, ?string $cursor = null): ?ClustersInstancesHeartbeatsListResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/clusters/instances/{instanceId}/heartbeats', ['instanceId' => $this->serializePathParameter($instanceId, new PathParameterSpec('instanceId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ClustersInstancesHeartbeatsListResponse::fromArray($result) : null;
    }

    /** Enqueue a peer message to one instance or broadcast to online members */
    public function clustersMessagesCreate(array|EnqueueClusterPeerMessagesRequest $body, string $idempotencyKey): ?ClustersMessagesCreateResponse201
    {
        $path = '/backend/v3/api/clusters/messages';
        $payload = $body instanceof EnqueueClusterPeerMessagesRequest ? $body->toArray() : $body;
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
        return is_array($result) ? ClustersMessagesCreateResponse201::fromArray($result) : null;
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
