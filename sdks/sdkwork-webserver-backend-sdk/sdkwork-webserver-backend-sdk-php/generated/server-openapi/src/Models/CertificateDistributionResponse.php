<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CertificateDistributionResponse
{
    public ?string $serverId = null;

    public ?string $serverName = null;

    public ?string $host = null;

    public ?string $desiredSyncVersion = null;

    public ?string $appliedSyncVersion = null;

    public ?string $status = null;

    public ?string $lastHeartbeatAt = null;

    public function __construct(array $data = [])
    {
        $this->serverId = array_key_exists('serverId', $data)
            ? $data['serverId']
            : null;
        $this->serverName = array_key_exists('serverName', $data)
            ? $data['serverName']
            : null;
        $this->host = array_key_exists('host', $data)
            ? $data['host']
            : null;
        $this->desiredSyncVersion = array_key_exists('desiredSyncVersion', $data)
            ? $data['desiredSyncVersion']
            : null;
        $this->appliedSyncVersion = array_key_exists('appliedSyncVersion', $data)
            ? $data['appliedSyncVersion']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->lastHeartbeatAt = array_key_exists('lastHeartbeatAt', $data)
            ? $data['lastHeartbeatAt']
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
            'serverName' => $this->serverName,
            'host' => $this->host,
            'desiredSyncVersion' => $this->desiredSyncVersion,
            'appliedSyncVersion' => $this->appliedSyncVersion,
            'status' => $this->status,
            'lastHeartbeatAt' => $this->lastHeartbeatAt,
        ];
    }
}
