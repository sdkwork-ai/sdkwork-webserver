<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Api;

use SDKWork\Webserver\BackendSdk\Models\ApplicationsDeploymentsListResponse;

final class ApplicationDeploymentApi extends BaseApi
{
    /** List application deployments */
    public function applicationsDeploymentsList(string $applicationId, ?int $pageSize = null, ?string $cursor = null, ?int $status = null): ?ApplicationsDeploymentsListResponse
    {
        $path = $this->interpolatePath('/backend/v3/api/applications/{applicationId}/deployments', ['applicationId' => $this->serializePathParameter($applicationId, new PathParameterSpec('applicationId', 'simple', false))]);
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
            new QueryParameterSpec('status', $status, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? ApplicationsDeploymentsListResponse::fromArray($result) : null;
    }

}
