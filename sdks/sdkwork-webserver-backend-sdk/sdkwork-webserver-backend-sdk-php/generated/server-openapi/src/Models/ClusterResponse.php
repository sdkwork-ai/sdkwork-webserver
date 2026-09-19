<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterResponse
{
    public ?string $id = null;

    public ?string $name = null;

    public ?string $code = null;

    public ?string $description = null;

    /** 0=inactive, 1=active */
    public ?int $status = null;

    public ?int $heartbeatIntervalSeconds = null;

    public ?int $offlineThresholdSeconds = null;

    public ?string $hostCount = null;

    public ?string $instanceCount = null;

    public ?string $onlineInstanceCount = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->code = array_key_exists('code', $data)
            ? $data['code']
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
        $this->hostCount = array_key_exists('hostCount', $data)
            ? $data['hostCount']
            : null;
        $this->instanceCount = array_key_exists('instanceCount', $data)
            ? $data['instanceCount']
            : null;
        $this->onlineInstanceCount = array_key_exists('onlineInstanceCount', $data)
            ? $data['onlineInstanceCount']
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
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
            'name' => $this->name,
            'code' => $this->code,
            'description' => $this->description,
            'status' => $this->status,
            'heartbeatIntervalSeconds' => $this->heartbeatIntervalSeconds,
            'offlineThresholdSeconds' => $this->offlineThresholdSeconds,
            'hostCount' => $this->hostCount,
            'instanceCount' => $this->instanceCount,
            'onlineInstanceCount' => $this->onlineInstanceCount,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
