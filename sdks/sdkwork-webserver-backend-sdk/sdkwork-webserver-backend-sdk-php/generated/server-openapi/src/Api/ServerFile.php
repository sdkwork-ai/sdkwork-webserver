<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\ServerFilesNodeBrowseResponse;
use SDKWork\Web\BackendSdk\Models\ServerFilesNodeOperationsCreateResponse201;
use SDKWork\Web\BackendSdk\Models\ServerFilesNodeOperationsListResponse;
use SDKWork\Web\BackendSdk\Models\ServerFilesNodeReadResponse;
use SDKWork\Web\BackendSdk\Models\ServerFilesNodesListResponse;
use SDKWork\Web\BackendSdk\Models\ServerRunOperationRequest;

final class ServerFileApi extends BaseApi
{
    /** List Server Files deployment nodes */
    public function serverFilesNodesList(): ?ServerFilesNodesListResponse
    {
        $path = '/backend/v3/api/server_files/nodes';
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ServerFilesNodesListResponse::fromArray($result) : null;
    }

    /** Browse a deployment node directory */
    public function serverFilesNodeBrowse(string $nodeId, string $path_): ?ServerFilesNodeBrowseResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/server_files/nodes/{nodeId}/browse', ['nodeId' => $this->serializePathParameter($nodeId, new PathParameterSpec('nodeId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('path', $path_, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ServerFilesNodeBrowseResponse::fromArray($result) : null;
    }

    /** Read a text file on a deployment node */
    public function serverFilesNodeRead(string $nodeId, string $path_): ?ServerFilesNodeReadResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/server_files/nodes/{nodeId}/read', ['nodeId' => $this->serializePathParameter($nodeId, new PathParameterSpec('nodeId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('path', $path_, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ServerFilesNodeReadResponse::fromArray($result) : null;
    }

    /** List operations available for a project directory */
    public function serverFilesNodeOperationsList(string $nodeId, string $path_): ?ServerFilesNodeOperationsListResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/server_files/nodes/{nodeId}/operations', ['nodeId' => $this->serializePathParameter($nodeId, new PathParameterSpec('nodeId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('path', $path_, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ServerFilesNodeOperationsListResponse::fromArray($result) : null;
    }

    /** Run a project operation on a deployment node */
    public function serverFilesNodeOperationsCreate(string $nodeId, array|ServerRunOperationRequest $body): ?ServerFilesNodeOperationsCreateResponse201
    {
        $path = $this->interpolatePath('/backend/v3/api/server_files/nodes/{nodeId}/operations', ['nodeId' => $this->serializePathParameter($nodeId, new PathParameterSpec('nodeId', 'simple', false))]);
        $payload = $body instanceof ServerRunOperationRequest ? $body->toArray() : $body;
        $result = $this->client->request('POST', $path, [
            'json' => $payload,
        ]);
        return is_array($result) ? ServerFilesNodeOperationsCreateResponse201::fromArray($result) : null;
    }

}
