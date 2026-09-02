<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class AgentHeartbeatResponse
{
    public ?string $serverId = null;

    public ?int $status = null;

    public ?string $acknowledgedAt = null;

    public function __construct(array $data = [])
    {
        $this->serverId = array_key_exists('serverId', $data)
            ? $data['serverId']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->acknowledgedAt = array_key_exists('acknowledgedAt', $data)
            ? $data['acknowledgedAt']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'serverId' => $this->serverId,
            'status' => $this->status,
            'acknowledgedAt' => $this->acknowledgedAt,
        ];
    }
}
