<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\MetricsWindowValue;

/**
 * One metric across every window. `metric` is one of the entity ids (`users`, `tenants`, `applications`, `agents`), one of the storage ids (`storage.used_bytes`, `storage.object_count`), or a metered dimension (`traffic.requests`, …), which is left open on the same terms as the traffic readings: a dimension this contract has not heard of reaches the surface instead of failing the response.
 */
final class MetricsMetricTotals
{
    public ?string $metric = null;

    public ?string $unit = null;

    /** One entry per window, in the contract's window order. Reported in full rather than sparsely: a window that summed nothing is a real `0`, and a missing entry would leave a surface guessing between "zero" and "not measured". */
    public array $values = [];

    public function __construct(array $data = [])
    {
        $this->metric = array_key_exists('metric', $data)
            ? $data['metric']
            : null;
        $this->unit = array_key_exists('unit', $data)
            ? $data['unit']
            : null;
        $this->values = array_key_exists('values', $data)
            ? is_array($data['values'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MetricsWindowValue::fromArray($item) : $item, $data['values']))
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
            'values' => array_values(array_map(static fn($item) => $item instanceof MetricsWindowValue ? $item->toArray() : $item, $this->values)),
        ];
    }
}
