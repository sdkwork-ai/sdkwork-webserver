<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ServerEntry
{
    public ?string $name = null;

    public ?string $kind = null;

    public ?string $path = null;

    public ?string $size = null;

    public ?string $projectType = null;

    public ?bool $isProjectRoot = null;

    public function __construct(array $data = [])
    {
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->kind = array_key_exists('kind', $data)
            ? $data['kind']
            : null;
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->size = array_key_exists('size', $data)
            ? $data['size']
            : null;
        $this->projectType = array_key_exists('projectType', $data)
            ? $data['projectType']
            : null;
        $this->isProjectRoot = array_key_exists('isProjectRoot', $data)
            ? $data['isProjectRoot']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'name' => $this->name,
            'kind' => $this->kind,
            'path' => $this->path,
            'size' => $this->size,
            'projectType' => $this->projectType,
            'isProjectRoot' => $this->isProjectRoot,
        ];
    }
}
