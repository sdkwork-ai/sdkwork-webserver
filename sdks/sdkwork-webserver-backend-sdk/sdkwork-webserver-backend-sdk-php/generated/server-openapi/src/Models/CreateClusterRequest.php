<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class CreateClusterRequest
{
    public ?string $name = null;

    public ?string $code = null;

    public ?string $description = null;

    public ?int $heartbeatIntervalSeconds = null;

    public ?int $offlineThresholdSeconds = null;

    /** Request routing strategy; defaults to `round_robin` when omitted. */
    public ?string $lbStrategy = null;

    /** Service domains auto-routed to this cluster's instances. */
    public array $servedDomains = [];

    public function __construct(array $data = [])
    {
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->code = array_key_exists('code', $data)
            ? $data['code']
            : null;
        $this->description = array_key_exists('description', $data)
            ? $data['description']
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
            'code' => $this->code,
            'description' => $this->description,
            'heartbeatIntervalSeconds' => $this->heartbeatIntervalSeconds,
            'offlineThresholdSeconds' => $this->offlineThresholdSeconds,
            'lbStrategy' => $this->lbStrategy,
            'servedDomains' => array_values(array_map(static fn($item) => $item, $this->servedDomains)),
        ];
    }
}
