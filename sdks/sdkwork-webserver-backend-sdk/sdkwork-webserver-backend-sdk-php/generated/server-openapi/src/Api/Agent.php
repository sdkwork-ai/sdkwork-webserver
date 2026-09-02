<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\AgentHeartbeatRequest;
use SDKWork\Web\BackendSdk\Models\HeartbeatResponse;
use SDKWork\Web\BackendSdk\Models\RetrieveResponse;

final class AgentApi extends BaseApi
{
    /** Report an edge-agent heartbeat */
    public function heartbeat(array|AgentHeartbeatRequest $body): ?HeartbeatResponse
    {
        $path = '/backend/v3/api/agent/heartbeat';
        $payload = $body instanceof AgentHeartbeatRequest ? $body->toArray() : $body;
        $result = $this->client->request('POST', $path, [
            'json' => $payload,
        ]);
        return is_array($result) ? HeartbeatResponse::fromArray($result) : null;
    }

    /** Retrieve the Nginx configuration and certificate bundle */
    public function retrieve(?string $ifSyncVersion = null): ?RetrieveResponse
    {
        $path = '/backend/v3/api/agent/sync';
        $query = $this->buildQueryString([
            new QueryParameterSpec('if_sync_version', $ifSyncVersion, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? RetrieveResponse::fromArray($result) : null;
    }

}
