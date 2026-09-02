<?php

declare(strict_types=1);

namespace SDKWork\Webserver\AppSdk\Models;

use SDKWork\Webserver\AppSdk\Models\ApplicationResponse;

final class ApplicationPage
{
    public array $items = [];

    /** Total item count as a string to avoid JavaScript precision loss. */
    public ?string $total = null;

    public ?int $page = null;

    public ?int $pageSize = null;

    public function __construct(array $data = [])
    {
        $this->items = array_key_exists('items', $data)
            ? is_array($data['items'])
                ? array_values(array_map(static fn($item) => is_array($item) ? ApplicationResponse::fromArray($item) : $item, $data['items']))
                : []
            : [];
        $this->total = array_key_exists('total', $data)
            ? $data['total']
            : null;
        $this->page = array_key_exists('page', $data)
            ? $data['page']
            : null;
        $this->pageSize = array_key_exists('pageSize', $data)
            ? $data['pageSize']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'items' => array_values(array_map(static fn($item) => $item instanceof ApplicationResponse ? $item->toArray() : $item, $this->items)),
            'total' => $this->total,
            'page' => $this->page,
            'pageSize' => $this->pageSize,
        ];
    }
}
