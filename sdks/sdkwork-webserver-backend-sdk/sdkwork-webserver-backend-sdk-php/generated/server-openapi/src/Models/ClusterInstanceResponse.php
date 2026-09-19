<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterInstanceResponse
{
    public ?string $id = null;

    public ?string $clusterId = null;

    public ?string $hostId = null;

    public ?string $hostName = null;

    public ?string $name = null;

    public ?string $role = null;

    public ?string $environment = null;

    public ?int $processPid = null;

    public ?string $processStartedAt = null;

    public ?string $bindHost = null;

    public ?int $bindPort = null;

    public ?string $publicEndpoint = null;

    public ?string $buildVersion = null;

    /** 0=offline, 1=online, 2=starting, 3=stopping, 4=error, 5=maintenance */
    public ?int $status = null;

    public ?string $healthState = null;

    public ?string $lastHeartbeatAt = null;

    public ?string $lastOnlineAt = null;

    public ?string $uptimeSeconds = null;

    /** Latest resource metrics snapshot (CPU/memory/connections). */
    public array $metrics = [];

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->clusterId = array_key_exists('clusterId', $data)
            ? $data['clusterId']
            : null;
        $this->hostId = array_key_exists('hostId', $data)
            ? $data['hostId']
            : null;
        $this->hostName = array_key_exists('hostName', $data)
            ? $data['hostName']
            : null;
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->role = array_key_exists('role', $data)
            ? $data['role']
            : null;
        $this->environment = array_key_exists('environment', $data)
            ? $data['environment']
            : null;
        $this->processPid = array_key_exists('processPid', $data)
            ? $data['processPid']
            : null;
        $this->processStartedAt = array_key_exists('processStartedAt', $data)
            ? $data['processStartedAt']
            : null;
        $this->bindHost = array_key_exists('bindHost', $data)
            ? $data['bindHost']
            : null;
        $this->bindPort = array_key_exists('bindPort', $data)
            ? $data['bindPort']
            : null;
        $this->publicEndpoint = array_key_exists('publicEndpoint', $data)
            ? $data['publicEndpoint']
            : null;
        $this->buildVersion = array_key_exists('buildVersion', $data)
            ? $data['buildVersion']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->healthState = array_key_exists('healthState', $data)
            ? $data['healthState']
            : null;
        $this->lastHeartbeatAt = array_key_exists('lastHeartbeatAt', $data)
            ? $data['lastHeartbeatAt']
            : null;
        $this->lastOnlineAt = array_key_exists('lastOnlineAt', $data)
            ? $data['lastOnlineAt']
            : null;
        $this->uptimeSeconds = array_key_exists('uptimeSeconds', $data)
            ? $data['uptimeSeconds']
            : null;
        $this->metrics = array_key_exists('metrics', $data)
            ? is_array($data['metrics']) ? $data['metrics'] : []
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
            'clusterId' => $this->clusterId,
            'hostId' => $this->hostId,
            'hostName' => $this->hostName,
            'name' => $this->name,
            'role' => $this->role,
            'environment' => $this->environment,
            'processPid' => $this->processPid,
            'processStartedAt' => $this->processStartedAt,
            'bindHost' => $this->bindHost,
            'bindPort' => $this->bindPort,
            'publicEndpoint' => $this->publicEndpoint,
            'buildVersion' => $this->buildVersion,
            'status' => $this->status,
            'healthState' => $this->healthState,
            'lastHeartbeatAt' => $this->lastHeartbeatAt,
            'lastOnlineAt' => $this->lastOnlineAt,
            'uptimeSeconds' => $this->uptimeSeconds,
            'metrics' => $this->metrics,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
