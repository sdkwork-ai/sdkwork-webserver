<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class DomainDeploymentResponse
{
    public ?string $id = null;

    public ?int $status = null;

    public ?string $environment = null;

    public ?string $versionTag = null;

    public ?string $completedAt = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->environment = array_key_exists('environment', $data)
            ? $data['environment']
            : null;
        $this->versionTag = array_key_exists('versionTag', $data)
            ? $data['versionTag']
            : null;
        $this->completedAt = array_key_exists('completedAt', $data)
            ? $data['completedAt']
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
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
            'status' => $this->status,
            'environment' => $this->environment,
            'versionTag' => $this->versionTag,
            'completedAt' => $this->completedAt,
            'createdAt' => $this->createdAt,
        ];
    }
}
