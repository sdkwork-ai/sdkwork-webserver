<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class TrafficUsageTotal
{
    public ?string $dimension = null;

    public ?string $quantity = null;

    public ?string $unit = null;

    public function __construct(array $data = [])
    {
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
            'dimension' => $this->dimension,
            'quantity' => $this->quantity,
            'unit' => $this->unit,
        ];
    }
}
