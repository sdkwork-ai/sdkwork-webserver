<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

/**
 * One app's aggregate over the window. A row carrying neither `appUuid` nor `appSlug` is the **unattributed** bucket: traffic served for a hostname the edge could not resolve to an app. It is reported rather than dropped so the per-app rows keep summing back to the total; a surface must render it as its own row, or the breakdown appears to lose traffic.
 */
final class TrafficUsageAppTotal
{
    public ?string $appUuid = null;

    public ?string $appSlug = null;

    public ?string $dimension = null;

    public ?string $quantity = null;

    public ?string $unit = null;

    public function __construct(array $data = [])
    {
        $this->appUuid = array_key_exists('appUuid', $data)
            ? $data['appUuid']
            : null;
        $this->appSlug = array_key_exists('appSlug', $data)
            ? $data['appSlug']
            : null;
        $this->dimension = array_key_exists('dimension', $data)
            ? $data['dimension']
            : null;
        $this->quantity = array_key_exists('quantity', $data)
            ? $data['quantity']
            : null;
        $this->unit = array_key_exists('unit', $data)
            ? $data['unit']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'appUuid' => $this->appUuid,
            'appSlug' => $this->appSlug,
            'dimension' => $this->dimension,
            'quantity' => $this->quantity,
            'unit' => $this->unit,
        ];
    }
}
