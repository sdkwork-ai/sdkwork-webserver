<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class TrafficUsageDailyPoint
{
    public ?string $usageDate = null;

    public ?string $dimension = null;

    public ?string $quantity = null;

    public function __construct(array $data = [])
    {
        $this->usageDate = array_key_exists('usageDate', $data)
            ? $data['usageDate']
            : null;
        $this->dimension = array_key_exists('dimension', $data)
            ? $data['dimension']
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
            'usageDate' => $this->usageDate,
            'dimension' => $this->dimension,
            'quantity' => $this->quantity,
        ];
    }
}
