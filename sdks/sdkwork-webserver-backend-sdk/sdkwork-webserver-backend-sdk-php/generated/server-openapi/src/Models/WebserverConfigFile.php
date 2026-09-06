<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class WebserverConfigFile
{
    public ?string $id = null;

    public ?string $kind = null;

    public ?string $name = null;

    public ?string $path = null;

    public ?string $language = null;

    public ?bool $writable = null;

    /** Decoded text content, bounded by the read size limit. */
    public ?string $content = null;

    public ?string $size = null;

    /** SHA-256 hex digest of the on-disk bytes. */
    public ?string $sha256 = null;

    /** Last modification time, Unix seconds. */
    public ?string $updatedAt = null;

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
        $this->writable = array_key_exists('writable', $data)
            ? $data['writable']
            : null;
        $this->content = array_key_exists('content', $data)
            ? $data['content']
            : null;
        $this->size = array_key_exists('size', $data)
            ? $data['size']
            : null;
        $this->sha256 = array_key_exists('sha256', $data)
            ? $data['sha256']
            : null;
        $this->updatedAt = array_key_exists('updatedAt', $data)
            ? $data['updatedAt']
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
            'writable' => $this->writable,
            'content' => $this->content,
            'size' => $this->size,
            'sha256' => $this->sha256,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
