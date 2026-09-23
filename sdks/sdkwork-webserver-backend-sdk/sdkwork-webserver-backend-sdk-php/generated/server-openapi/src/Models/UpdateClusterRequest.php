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

    /** Request routing strategy across the cluster's instances. */
    public ?string $lbStrategy = null;

    /** Service domains auto-routed to this cluster's instances; replaces the whole list when present. */
    public array $servedDomains = [];

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
        $this->lbStrategy = array_key_exists('lbStrategy', $data)
            ? $data['lbStrategy']
            : null;
        $this->servedDomains = array_key_exists('servedDomains', $data)
            ? is_array($data['servedDomains'])
                ? array_values(array_map(static fn($item) => $item, $data['servedDomains']))
                : []
            : [];
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
            'lbStrategy' => $this->lbStrategy,
            'servedDomains' => array_values(array_map(static fn($item) => $item, $this->servedDomains)),
        ];
    }
}
