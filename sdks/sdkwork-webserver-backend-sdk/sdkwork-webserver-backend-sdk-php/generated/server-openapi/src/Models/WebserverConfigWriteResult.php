<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class WebserverConfigWriteResult
{
    public ?string $id = null;

    public ?string $path = null;

    public ?string $size = null;

    /** SHA-256 hex digest of the written content. */
    public ?string $sha256 = null;

    /** Backup file holding the previous content, when an existing file was overwritten. */
    public ?string $backupPath = null;

    /** Modification time of the written file, Unix seconds. */
    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->size = array_key_exists('size', $data)
            ? $data['size']
            : null;
        $this->sha256 = array_key_exists('sha256', $data)
            ? $data['sha256']
            : null;
        $this->backupPath = array_key_exists('backupPath', $data)
            ? $data['backupPath']
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
            'path' => $this->path,
            'size' => $this->size,
            'sha256' => $this->sha256,
            'backupPath' => $this->backupPath,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
