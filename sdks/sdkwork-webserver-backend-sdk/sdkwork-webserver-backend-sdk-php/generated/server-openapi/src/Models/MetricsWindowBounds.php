<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class MetricsWindowBounds
{
    public ?string $window = null;

    /** Inclusive UTC day. Absent for the lifetime window, whose lower bound is wherever the figures begin rather than a day this contract could invent. */
    public ?string $dateFrom = null;

    public ?string $dateTo = null;

    public function __construct(array $data = [])
    {
        $this->window = array_key_exists('window', $data)
            ? $data['window']
            : null;
        $this->dateFrom = array_key_exists('dateFrom', $data)
            ? $data['dateFrom']
            : null;
        $this->dateTo = array_key_exists('dateTo', $data)
            ? $data['dateTo']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'window' => $this->window,
            'dateFrom' => $this->dateFrom,
            'dateTo' => $this->dateTo,
        ];
    }
}
