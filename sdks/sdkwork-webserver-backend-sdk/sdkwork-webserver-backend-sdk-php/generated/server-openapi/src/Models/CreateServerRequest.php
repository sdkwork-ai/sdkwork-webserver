<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CreateServerRequest
{
    public ?string $name = null;

    public ?string $host = null;

    /** Irreversible tenant scope bound to runtime-set delivery for this node. */
    public ?string $tenantScopeHash = null;

    public ?int $sshPort = null;

    public function __construct(array $data = [])
    {
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
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'name' => $this->name,
            'host' => $this->host,
            'tenantScopeHash' => $this->tenantScopeHash,
            'sshPort' => $this->sshPort,
        ];
    }
}
