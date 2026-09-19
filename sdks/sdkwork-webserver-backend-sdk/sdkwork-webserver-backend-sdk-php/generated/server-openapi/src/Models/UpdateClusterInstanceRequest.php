<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class UpdateClusterInstanceRequest
{
    public ?string $name = null;

    public ?int $status = null;

    public ?string $publicEndpoint = null;

    public function __construct(array $data = [])
    {
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->publicEndpoint = array_key_exists('publicEndpoint', $data)
            ? $data['publicEndpoint']
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
            'status' => $this->status,
            'publicEndpoint' => $this->publicEndpoint,
        ];
    }
}
