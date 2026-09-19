<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterOverviewResponse
{
    public ?string $totalHosts = null;

    public ?string $onlineHosts = null;

    public ?string $totalInstances = null;

    public ?string $onlineInstances = null;

    public ?string $unhealthyInstances = null;

    public ?string $pendingPeerMessages = null;

    public ?string $generatedAt = null;

    public function __construct(array $data = [])
    {
        $this->totalHosts = array_key_exists('totalHosts', $data)
            ? $data['totalHosts']
            : null;
        $this->onlineHosts = array_key_exists('onlineHosts', $data)
            ? $data['onlineHosts']
            : null;
        $this->totalInstances = array_key_exists('totalInstances', $data)
            ? $data['totalInstances']
            : null;
        $this->onlineInstances = array_key_exists('onlineInstances', $data)
            ? $data['onlineInstances']
            : null;
        $this->unhealthyInstances = array_key_exists('unhealthyInstances', $data)
            ? $data['unhealthyInstances']
            : null;
        $this->pendingPeerMessages = array_key_exists('pendingPeerMessages', $data)
            ? $data['pendingPeerMessages']
            : null;
        $this->generatedAt = array_key_exists('generatedAt', $data)
            ? $data['generatedAt']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'totalHosts' => $this->totalHosts,
            'onlineHosts' => $this->onlineHosts,
            'totalInstances' => $this->totalInstances,
            'onlineInstances' => $this->onlineInstances,
            'unhealthyInstances' => $this->unhealthyInstances,
            'pendingPeerMessages' => $this->pendingPeerMessages,
            'generatedAt' => $this->generatedAt,
        ];
    }
}
