<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

/**
 * The window the per-day series was cut against, as the server resolved it. Reported because the request's bounds are optional: a surface that labelled the series from its own guess at the default would name a period the points do not cover. Deliberately separate from the card windows, which answer a different question and may not coincide with this one.
 */
final class MetricsSeriesWindow
{
    /** Inclusive UTC day. */
    public ?string $dateFrom = null;

    /** **Exclusive** UTC day. */
    public ?string $dateTo = null;

    public function __construct(array $data = [])
    {
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
            'dateFrom' => $this->dateFrom,
            'dateTo' => $this->dateTo,
        ];
    }
}
