<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterSyncManifest
{
    public ?string $clusterId = null;

    public ?string $kind = null;

    public ?string $revision = null;

    /** Canonical payload digest the node verifies before applying. */
    public ?string $sha256 = null;

    /** Track-specific desired-state payload; opaque to the transport. */
    public array $payload = [];

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->clusterId = array_key_exists('clusterId', $data)
            ? $data['clusterId']
            : null;
        $this->kind = array_key_exists('kind', $data)
            ? $data['kind']
            : null;
        $this->revision = array_key_exists('revision', $data)
            ? $data['revision']
            : null;
        $this->sha256 = array_key_exists('sha256', $data)
            ? $data['sha256']
            : null;
        $this->payload = array_key_exists('payload', $data)
            ? is_array($data['payload']) ? $data['payload'] : []
            : [];
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'clusterId' => $this->clusterId,
            'kind' => $this->kind,
            'revision' => $this->revision,
            'sha256' => $this->sha256,
            'payload' => $this->payload,
            'createdAt' => $this->createdAt,
        ];
    }
}
