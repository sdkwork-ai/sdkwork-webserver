<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\TrafficUsageAppTotal;
use SDKWork\Webserver\BackendSdk\Models\TrafficUsageDailyPoint;
use SDKWork\Webserver\BackendSdk\Models\TrafficUsageTenantTotal;
use SDKWork\Webserver\BackendSdk\Models\TrafficUsageTotal;

/**
 * Aggregated traffic usage over a **half-open** date window (`dateFrom <= day < dateTo`). Every view is derived from the same append-only traffic facts, so the totals, the daily series, and the per-app breakdown are consistent with each other by construction rather than by a reconciliation job having run most recently.
 */
final class TrafficUsageStatisticsResponse
{
    public ?string $dateFrom = null;

    public ?string $dateTo = null;

    /** Whether the figures cover every tenant rather than the caller's own. Reported so a surface cannot render a platform-wide number as if it were the caller's own, or the reverse. */
    public ?bool $platformScope = null;

    public array $totals = [];

    public array $daily = [];

    public array $apps = [];

    /** Per-tenant breakdown. Empty for a tenant-scoped read, where the answer would be the caller's own totals repeated once per dimension. */
    public array $tenants = [];

    public function __construct(array $data = [])
    {
        $this->dateFrom = array_key_exists('dateFrom', $data)
            ? $data['dateFrom']
            : null;
        $this->dateTo = array_key_exists('dateTo', $data)
            ? $data['dateTo']
            : null;
        $this->platformScope = array_key_exists('platformScope', $data)
            ? $data['platformScope']
            : null;
        $this->totals = array_key_exists('totals', $data)
            ? is_array($data['totals'])
                ? array_values(array_map(static fn($item) => is_array($item) ? TrafficUsageTotal::fromArray($item) : $item, $data['totals']))
                : []
            : [];
        $this->daily = array_key_exists('daily', $data)
            ? is_array($data['daily'])
                ? array_values(array_map(static fn($item) => is_array($item) ? TrafficUsageDailyPoint::fromArray($item) : $item, $data['daily']))
                : []
            : [];
        $this->apps = array_key_exists('apps', $data)
            ? is_array($data['apps'])
                ? array_values(array_map(static fn($item) => is_array($item) ? TrafficUsageAppTotal::fromArray($item) : $item, $data['apps']))
                : []
            : [];
        $this->tenants = array_key_exists('tenants', $data)
            ? is_array($data['tenants'])
                ? array_values(array_map(static fn($item) => is_array($item) ? TrafficUsageTenantTotal::fromArray($item) : $item, $data['tenants']))
                : []
            : [];
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'dateFrom' => $this->dateFrom,
            'dateTo' => $this->dateTo,
            'platformScope' => $this->platformScope,
            'totals' => array_values(array_map(static fn($item) => $item instanceof TrafficUsageTotal ? $item->toArray() : $item, $this->totals)),
            'daily' => array_values(array_map(static fn($item) => $item instanceof TrafficUsageDailyPoint ? $item->toArray() : $item, $this->daily)),
            'apps' => array_values(array_map(static fn($item) => $item instanceof TrafficUsageAppTotal ? $item->toArray() : $item, $this->apps)),
            'tenants' => array_values(array_map(static fn($item) => $item instanceof TrafficUsageTenantTotal ? $item->toArray() : $item, $this->tenants)),
        ];
    }
}
