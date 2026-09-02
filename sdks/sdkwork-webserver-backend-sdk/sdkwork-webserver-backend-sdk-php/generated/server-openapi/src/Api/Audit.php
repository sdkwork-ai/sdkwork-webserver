<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\AuditLogsListResponse;

final class AuditApi extends BaseApi
{
    /** List audit logs */
    public function logsList(?int $pageSize = null, ?string $cursor = null, ?string $targetType = null, ?string $action = null, ?string $operatorId = null, ?string $startDate = null, ?string $endDate = null): ?AuditLogsListResponse
    {
        $path = '/backend/v3/api/audit_logs';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
            new QueryParameterSpec('cursor', $cursor, 'form', true, false, null),
            new QueryParameterSpec('target_type', $targetType, 'form', true, false, null),
            new QueryParameterSpec('action', $action, 'form', true, false, null),
            new QueryParameterSpec('operator_id', $operatorId, 'form', true, false, null),
            new QueryParameterSpec('start_date', $startDate, 'form', true, false, null),
            new QueryParameterSpec('end_date', $endDate, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? AuditLogsListResponse::fromArray($result) : null;
    }

}
