<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\MetricsSeriesPoint;

/**
 * One entity metric's per-day arrivals over `MetricsSeriesWindow`, days ascending. Points are **sparse**: a day the metric gained nothing may carry no point at all, because a `GROUP BY` cannot report a day it never saw. A day inside the window with no point is a real `0`; emitting an explicit zero for every day would make the response's size a function of the window rather than of the data.
 */
final class MetricsSeries
{
    public ?string $metric = null;

    public ?string $unit = null;

    public array $points = [];

    public function __construct(array $data = [])
    {
        $this->metric = array_key_exists('metric', $data)
            ? $data['metric']
            : null;
        $this->unit = array_key_exists('unit', $data)
            ? $data['unit']
            : null;
        $this->points = array_key_exists('points', $data)
            ? is_array($data['points'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MetricsSeriesPoint::fromArray($item) : $item, $data['points']))
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
            'metric' => $this->metric,
            'unit' => $this->unit,
            'points' => array_values(array_map(static fn($item) => $item instanceof MetricsSeriesPoint ? $item->toArray() : $item, $this->points)),
        ];
    }
}
