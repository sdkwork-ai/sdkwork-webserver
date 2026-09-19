<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class UpdateClusterRequest
{
    public ?string $name = null;

    public ?string $description = null;

    public ?int $status = null;

    public ?int $heartbeatIntervalSeconds = null;

    public ?int $offlineThresholdSeconds = null;

    public function __construct(array $data = [])
    {
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->description = array_key_exists('description', $data)
            ? $data['description']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->heartbeatIntervalSeconds = array_key_exists('heartbeatIntervalSeconds', $data)
            ? $data['heartbeatIntervalSeconds']
            : null;
        $this->offlineThresholdSeconds = array_key_exists('offlineThresholdSeconds', $data)
            ? $data['offlineThresholdSeconds']
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
            'description' => $this->description,
            'status' => $this->status,
            'heartbeatIntervalSeconds' => $this->heartbeatIntervalSeconds,
            'offlineThresholdSeconds' => $this->offlineThresholdSeconds,
        ];
    }
}
