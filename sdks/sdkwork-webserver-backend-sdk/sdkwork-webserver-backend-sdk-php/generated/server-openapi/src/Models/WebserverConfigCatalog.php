<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\PageInfo;
use SDKWork\Webserver\BackendSdk\Models\WebserverConfigEntry;

final class WebserverConfigCatalog
{
    /** Web Server runtime configuration root the catalog was enumerated from. */
    public ?string $configRoot = null;

    public array $items = [];

    /** True when the deployment has more catalog entries than the server's maximumEntries cap; items is the capped prefix. */
    public ?bool $truncated = null;

    /** Present only when page/pageSize were requested; omitted for the legacy whole-catalog response. */
    public ?PageInfo $pageInfo = null;

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
        $this->truncated = array_key_exists('truncated', $data)
            ? $data['truncated']
            : null;
        $this->pageInfo = array_key_exists('pageInfo', $data)
            ? is_array($data['pageInfo']) ? PageInfo::fromArray($data['pageInfo']) : null
            : null;
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
            'truncated' => $this->truncated,
            'pageInfo' => $this->pageInfo instanceof PageInfo ? $this->pageInfo->toArray() : $this->pageInfo,
        ];
    }
}
