<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

use SDKWork\Web\AppSdk\Models\PageInfo;

final class SdkWorkPageData
{
    public array $items = [];

    public ?PageInfo $pageInfo = null;

    public function __construct(array $data = [])
    {
        $this->items = array_key_exists('items', $data)
            ? is_array($data['items'])
                ? array_values(array_map(static fn($item) => is_array($item) ? $item : [], $data['items']))
                : []
            : [];
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
            'items' => array_values(array_map(static fn($item) => $item, $this->items)),
            'pageInfo' => $this->pageInfo instanceof PageInfo ? $this->pageInfo->toArray() : $this->pageInfo,
        ];
    }
}
