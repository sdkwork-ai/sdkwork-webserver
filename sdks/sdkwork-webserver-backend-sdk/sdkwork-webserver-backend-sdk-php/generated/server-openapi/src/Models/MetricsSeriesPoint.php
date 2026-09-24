<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class MetricsSeriesPoint
{
    /** Calendar day in UTC. */
    public ?string $date = null;

    public ?string $quantity = null;

    public function __construct(array $data = [])
    {
        $this->date = array_key_exists('date', $data)
            ? $data['date']
            : null;
        $this->quantity = array_key_exists('quantity', $data)
            ? $data['quantity']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'date' => $this->date,
            'quantity' => $this->quantity,
        ];
    }
}
