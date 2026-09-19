<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class UpdateClusterHostRequest
{
    public ?string $name = null;

    public ?string $clusterId = null;

    public function __construct(array $data = [])
    {
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->clusterId = array_key_exists('clusterId', $data)
            ? $data['clusterId']
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
            'clusterId' => $this->clusterId,
        ];
    }
}
