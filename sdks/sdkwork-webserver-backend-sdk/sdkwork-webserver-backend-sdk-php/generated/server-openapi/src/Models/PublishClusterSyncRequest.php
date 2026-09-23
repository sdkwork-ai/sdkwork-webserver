<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class PublishClusterSyncRequest
{
    /** Desired-state track to publish. */
    public ?string $kind = null;

    /** Track-specific desired-state payload; opaque to the transport. */
    public array $payload = [];

    public function __construct(array $data = [])
    {
        $this->kind = array_key_exists('kind', $data)
            ? $data['kind']
            : null;
        $this->payload = array_key_exists('payload', $data)
            ? is_array($data['payload']) ? $data['payload'] : []
            : [];
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'kind' => $this->kind,
            'payload' => $this->payload,
        ];
    }
}
