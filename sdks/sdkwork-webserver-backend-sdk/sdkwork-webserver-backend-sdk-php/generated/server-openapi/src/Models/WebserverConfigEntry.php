<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class WebserverConfigEntry
{
    /** Stable, content-independent catalog id (SHA-256 over the catalog identity). */
    public ?string $id = null;

    public ?string $kind = null;

    public ?string $name = null;

    /** Posix path relative to the owning group root. */
    public ?string $path = null;

    /** Editor language id for the file content. */
    public ?string $language = null;

    public ?string $size = null;

    /** Last modification time, Unix seconds. */
    public ?string $updatedAt = null;

    /** Module sidecar configs are read-only by ownership contract. */
    public ?bool $writable = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->kind = array_key_exists('kind', $data)
            ? $data['kind']
            : null;
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->language = array_key_exists('language', $data)
            ? $data['language']
            : null;
        $this->size = array_key_exists('size', $data)
            ? $data['size']
            : null;
        $this->updatedAt = array_key_exists('updatedAt', $data)
            ? $data['updatedAt']
            : null;
        $this->writable = array_key_exists('writable', $data)
            ? $data['writable']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'kind' => $this->kind,
            'name' => $this->name,
            'path' => $this->path,
            'language' => $this->language,
            'size' => $this->size,
            'updatedAt' => $this->updatedAt,
            'writable' => $this->writable,
        ];
    }
}
