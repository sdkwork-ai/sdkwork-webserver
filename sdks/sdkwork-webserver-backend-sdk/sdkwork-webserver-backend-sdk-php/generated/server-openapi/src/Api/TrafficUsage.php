<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Api;

use SDKWork\Webserver\BackendSdk\Models\PlatformTrafficUsagesRetrieveResponse;
use SDKWork\Webserver\BackendSdk\Models\TrafficUsagesRetrieveResponse;

final class TrafficUsageApi extends BaseApi
{
    /** Retrieve aggregated traffic usage of the caller's own tenant */
    public function trafficUsagesRetrieve(?string $dateFrom = null, ?string $dateTo = null, ?string $dimension = null, ?int $topApps = null): ?TrafficUsagesRetrieveResponse
    {
        $path = '/backend/v3/api/traffic_usage';
        $query = $this->buildQueryString([
            new QueryParameterSpec('date_from', $dateFrom, 'form', true, false, null),
            new QueryParameterSpec('date_to', $dateTo, 'form', true, false, null),
            new QueryParameterSpec('dimension', $dimension, 'form', true, false, null),
            new QueryParameterSpec('top_apps', $topApps, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? TrafficUsagesRetrieveResponse::fromArray($result) : null;
    }

    /** Retrieve aggregated traffic usage of every tenant */
    public function platformTrafficUsagesRetrieve(?string $dateFrom = null, ?string $dateTo = null, ?string $dimension = null, ?int $topApps = null): ?PlatformTrafficUsagesRetrieveResponse
    {
        $path = '/backend/v3/api/platform_traffic_usage';
        $query = $this->buildQueryString([
            new QueryParameterSpec('date_from', $dateFrom, 'form', true, false, null),
            new QueryParameterSpec('date_to', $dateTo, 'form', true, false, null),
            new QueryParameterSpec('dimension', $dimension, 'form', true, false, null),
            new QueryParameterSpec('top_apps', $topApps, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? PlatformTrafficUsagesRetrieveResponse::fromArray($result) : null;
    }

}
