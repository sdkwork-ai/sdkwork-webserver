<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CreateRootDomainRequest
{
    public ?string $hostname = null;

    public function __construct(array $data = [])
    {
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
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
        ];
    }
}
