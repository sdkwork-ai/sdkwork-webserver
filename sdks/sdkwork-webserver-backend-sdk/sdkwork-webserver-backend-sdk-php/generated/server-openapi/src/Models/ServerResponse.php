<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class ServerResponse
{
    public ?string $id = null;

    public ?string $name = null;

    public ?string $host = null;

    public ?string $tenantScopeHash = null;

    public ?int $sshPort = null;

    /** 0=offline, 1=online */
    public ?int $status = null;

    public ?string $lastHeartbeatAt = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->host = array_key_exists('host', $data)
            ? $data['host']
            : null;
        $this->tenantScopeHash = array_key_exists('tenantScopeHash', $data)
            ? $data['tenantScopeHash']
            : null;
        $this->sshPort = array_key_exists('sshPort', $data)
            ? $data['sshPort']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->lastHeartbeatAt = array_key_exists('lastHeartbeatAt', $data)
            ? $data['lastHeartbeatAt']
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
            'name' => $this->name,
            'host' => $this->host,
            'tenantScopeHash' => $this->tenantScopeHash,
            'sshPort' => $this->sshPort,
            'status' => $this->status,
            'lastHeartbeatAt' => $this->lastHeartbeatAt,
            'createdAt' => $this->createdAt,
        ];
    }
}
