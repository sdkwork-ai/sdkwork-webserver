<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Api;

use SDKWork\Webserver\BackendSdk\Models\MetricsSummariesRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\PlatformMetricsSummariesRetrieveResponse;

final class MetricsSummaryApi extends BaseApi
{
    /** Retrieve the dashboard metric summary of the caller's own tenant */
    public function metricsSummariesRetrieve(?string $dateFrom = null, ?string $dateTo = null): ?MetricsSummariesRetrieveResponse
    {
        $path = '/backend/v3/api/metrics_summaries';
        $query = $this->buildQueryString([
            new QueryParameterSpec('date_from', $dateFrom, 'form', true, false, null),
            new QueryParameterSpec('date_to', $dateTo, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? MetricsSummariesRetrieveResponse::fromArray($result) : null;
    }

    /** Retrieve the dashboard metric summary of every tenant */
    public function platformMetricsSummariesRetrieve(?string $dateFrom = null, ?string $dateTo = null): ?PlatformMetricsSummariesRetrieveResponse
    {
        $path = '/backend/v3/api/platform_metrics_summaries';
        $query = $this->buildQueryString([
            new QueryParameterSpec('date_from', $dateFrom, 'form', true, false, null),
            new QueryParameterSpec('date_to', $dateTo, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? PlatformMetricsSummariesRetrieveResponse::fromArray($result) : null;
    }

}
