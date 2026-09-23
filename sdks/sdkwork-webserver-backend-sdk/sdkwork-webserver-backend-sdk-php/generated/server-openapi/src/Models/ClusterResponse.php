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

    /** Request routing strategy across the cluster's instances. */
    public ?string $lbStrategy = null;

    /** Service domains auto-routed to this cluster's instances. */
    public array $servedDomains = [];

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
        $this->lbStrategy = array_key_exists('lbStrategy', $data)
            ? $data['lbStrategy']
            : null;
        $this->servedDomains = array_key_exists('servedDomains', $data)
            ? is_array($data['servedDomains'])
                ? array_values(array_map(static fn($item) => $item, $data['servedDomains']))
                : []
            : [];
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
            'lbStrategy' => $this->lbStrategy,
            'servedDomains' => array_values(array_map(static fn($item) => $item, $this->servedDomains)),
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
