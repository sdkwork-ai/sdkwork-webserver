<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Api;

use SDKWork\Web\BackendSdk\Models\CertificatesDistributionListResponse;

final class CertificateDistributionApi extends BaseApi
{
    /** List certificate manifest convergence by server */
    public function certificatesDistributionList(?int $page = null, ?int $pageSize = null): ?CertificatesDistributionListResponse
    {
        $path = '/backend/v3/api/certificate_distribution';
        $query = $this->buildQueryString([
            new QueryParameterSpec('page', $page, 'form', true, false, null),
            new QueryParameterSpec('page_size', $pageSize, 'form', true, false, null),
        ]);
        $path = $this->appendQueryString($path, $query);
        $result = $this->client->request('GET', $path, []);
        return is_array($result) ? CertificatesDistributionListResponse::fromArray($result) : null;
    }

}
