<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\ServerEntry;

final class ServerDirectoryListing
{
    public ?string $nodeId = null;

    public ?string $path = null;

    public ?string $parentPath = null;

    public array $entries = [];

    public function __construct(array $data = [])
    {
        $this->nodeId = array_key_exists('nodeId', $data)
            ? $data['nodeId']
            : null;
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->parentPath = array_key_exists('parentPath', $data)
            ? $data['parentPath']
            : null;
        $this->entries = array_key_exists('entries', $data)
            ? is_array($data['entries'])
                ? array_values(array_map(static fn($item) => is_array($item) ? ServerEntry::fromArray($item) : $item, $data['entries']))
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
            'nodeId' => $this->nodeId,
            'path' => $this->path,
            'parentPath' => $this->parentPath,
            'entries' => array_values(array_map(static fn($item) => $item instanceof ServerEntry ? $item->toArray() : $item, $this->entries)),
        ];
    }
}
