<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class UpdateClusterInstanceRequest
{
    public ?string $name = null;

    public ?int $status = null;

    public ?string $publicEndpoint = null;

    /** Cordon switch: `false` removes the instance from the routing pool while it keeps serving. */
    public ?bool $routingEnabled = null;

    /** Graceful drain start/clear. Starting a drain also cordons routing. */
    public ?bool $draining = null;

    /** Active-probe target override. */
    public ?string $probeUrl = null;

    /** Operator labels; replaces the whole map when present. */
    public array $labels = [];

    /** Per-instance load balancing weight override. */
    public ?int $routingWeight = null;

    /** Operator maintenance reason/context. */
    public ?string $maintenanceNote = null;

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
        $this->routingEnabled = array_key_exists('routingEnabled', $data)
            ? $data['routingEnabled']
            : null;
        $this->draining = array_key_exists('draining', $data)
            ? $data['draining']
            : null;
        $this->probeUrl = array_key_exists('probeUrl', $data)
            ? $data['probeUrl']
            : null;
        $this->labels = array_key_exists('labels', $data)
            ? is_array($data['labels'])
                ? array_map(static fn($item) => $item, $data['labels'])
                : []
            : [];
        $this->routingWeight = array_key_exists('routingWeight', $data)
            ? $data['routingWeight']
            : null;
        $this->maintenanceNote = array_key_exists('maintenanceNote', $data)
            ? $data['maintenanceNote']
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
            'routingEnabled' => $this->routingEnabled,
            'draining' => $this->draining,
            'probeUrl' => $this->probeUrl,
            'labels' => array_map(static fn($item) => $item, $this->labels),
            'routingWeight' => $this->routingWeight,
            'maintenanceNote' => $this->maintenanceNote,
        ];
    }
}
