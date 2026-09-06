<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\WebserverConfigEntry;

final class WebserverConfigCatalog
{
    /** Web Server runtime configuration root the catalog was enumerated from. */
    public ?string $configRoot = null;

    public array $items = [];

    public function __construct(array $data = [])
    {
        $this->configRoot = array_key_exists('configRoot', $data)
            ? $data['configRoot']
            : null;
        $this->items = array_key_exists('items', $data)
            ? is_array($data['items'])
                ? array_values(array_map(static fn($item) => is_array($item) ? WebserverConfigEntry::fromArray($item) : $item, $data['items']))
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
            'configRoot' => $this->configRoot,
            'items' => array_values(array_map(static fn($item) => $item instanceof WebserverConfigEntry ? $item->toArray() : $item, $this->items)),
        ];
    }
}
