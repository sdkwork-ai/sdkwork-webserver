<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class CreateDomainRequest
{
    public ?string $hostname = null;

    public ?bool $isPrimary = null;

    public ?bool $sslEnabled = null;

    public ?string $sslProvider = null;

    public function __construct(array $data = [])
    {
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
            : null;
        $this->isPrimary = array_key_exists('isPrimary', $data)
            ? $data['isPrimary']
            : null;
        $this->sslEnabled = array_key_exists('sslEnabled', $data)
            ? $data['sslEnabled']
            : null;
        $this->sslProvider = array_key_exists('sslProvider', $data)
            ? $data['sslProvider']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'hostname' => $this->hostname,
            'isPrimary' => $this->isPrimary,
            'sslEnabled' => $this->sslEnabled,
            'sslProvider' => $this->sslProvider,
        ];
    }
}
