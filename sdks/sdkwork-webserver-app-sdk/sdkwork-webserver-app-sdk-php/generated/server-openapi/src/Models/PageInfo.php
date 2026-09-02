<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class PageInfo
{
    public ?string $mode = null;

    public ?int $page = null;

    public ?int $pageSize = null;

    public ?string $totalItems = null;

    public ?int $totalPages = null;

    public ?string $nextCursor = null;

    public ?bool $hasMore = null;

    public function __construct(array $data = [])
    {
        $this->mode = array_key_exists('mode', $data)
            ? $data['mode']
            : null;
        $this->page = array_key_exists('page', $data)
            ? $data['page']
            : null;
        $this->pageSize = array_key_exists('pageSize', $data)
            ? $data['pageSize']
            : null;
        $this->totalItems = array_key_exists('totalItems', $data)
            ? $data['totalItems']
            : null;
        $this->totalPages = array_key_exists('totalPages', $data)
            ? $data['totalPages']
            : null;
        $this->nextCursor = array_key_exists('nextCursor', $data)
            ? $data['nextCursor']
            : null;
        $this->hasMore = array_key_exists('hasMore', $data)
            ? $data['hasMore']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'mode' => $this->mode,
            'page' => $this->page,
            'pageSize' => $this->pageSize,
            'totalItems' => $this->totalItems,
            'totalPages' => $this->totalPages,
            'nextCursor' => $this->nextCursor,
            'hasMore' => $this->hasMore,
        ];
    }
}
