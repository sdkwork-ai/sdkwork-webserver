<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\NginxConfigResponse;

final class NginxConfigPage
{
    public array $items = [];

    /** Total item count as a string to avoid JavaScript precision loss. */
    public ?string $total = null;

    public function __construct(array $data = [])
    {
        $this->items = array_key_exists('items', $data)
            ? is_array($data['items'])
                ? array_values(array_map(static fn($item) => is_array($item) ? NginxConfigResponse::fromArray($item) : $item, $data['items']))
                : []
            : [];
        $this->total = array_key_exists('total', $data)
            ? $data['total']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'items' => array_values(array_map(static fn($item) => $item instanceof NginxConfigResponse ? $item->toArray() : $item, $this->items)),
            'total' => $this->total,
        ];
    }
}
