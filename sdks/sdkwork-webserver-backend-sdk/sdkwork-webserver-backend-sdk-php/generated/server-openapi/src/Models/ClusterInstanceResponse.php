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

    /** `LAN` = same-subnet member; `TUNNEL` = API-only member reached through the reverse tunnel. */
    public ?string $joinMode = null;

    /** Service quality 0-100 derived from the latest heartbeat sample. */
    public ?int $qualityScore = null;

    /** Desired configuration revision; absent until the cluster publishes one. */
    public ?string $desiredConfigRevision = null;

    /** Configuration revision this instance last acknowledged as applied. */
    public ?string $appliedConfigRevision = null;

    /** Desired applications-manifest revision; absent until the cluster publishes one. */
    public ?string $desiredApplicationsRevision = null;

    /** Applications-manifest revision this instance last acknowledged as applied. */
    public ?string $appliedApplicationsRevision = null;

    /** Aggregate desired-vs-applied sync status for this instance. */
    public ?string $syncStatus = null;

    /** Cordon switch: `false` keeps the instance serving but removes it from the routing pool. */
    public ?bool $routingEnabled = null;

    /** Graceful drain in progress. */
    public ?bool $draining = null;

    /** Taken out of the routing pool by the active prober after consecutive failures. */
    public ?bool $ejected = null;

    /** Process restarts observed for this instance slot (auto-recovery evidence). */
    public ?int $restartCount = null;

    /** Operator labels. */
    public array $labels = [];

    /** Per-instance load balancing weight. */
    public ?int $routingWeight = null;

    /** Operator maintenance reason/context. */
    public ?string $maintenanceNote = null;

    /** Consecutive active-probe failures; reset on success. */
    public ?int $probeFailures = null;

    /** Active-probe target override. */
    public ?string $probeUrl = null;

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
        $this->joinMode = array_key_exists('joinMode', $data)
            ? $data['joinMode']
            : null;
        $this->qualityScore = array_key_exists('qualityScore', $data)
            ? $data['qualityScore']
            : null;
        $this->desiredConfigRevision = array_key_exists('desiredConfigRevision', $data)
            ? $data['desiredConfigRevision']
            : null;
        $this->appliedConfigRevision = array_key_exists('appliedConfigRevision', $data)
            ? $data['appliedConfigRevision']
            : null;
        $this->desiredApplicationsRevision = array_key_exists('desiredApplicationsRevision', $data)
            ? $data['desiredApplicationsRevision']
            : null;
        $this->appliedApplicationsRevision = array_key_exists('appliedApplicationsRevision', $data)
            ? $data['appliedApplicationsRevision']
            : null;
        $this->syncStatus = array_key_exists('syncStatus', $data)
            ? $data['syncStatus']
            : null;
        $this->routingEnabled = array_key_exists('routingEnabled', $data)
            ? $data['routingEnabled']
            : null;
        $this->draining = array_key_exists('draining', $data)
            ? $data['draining']
            : null;
        $this->ejected = array_key_exists('ejected', $data)
            ? $data['ejected']
            : null;
        $this->restartCount = array_key_exists('restartCount', $data)
            ? $data['restartCount']
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
        $this->probeFailures = array_key_exists('probeFailures', $data)
            ? $data['probeFailures']
            : null;
        $this->probeUrl = array_key_exists('probeUrl', $data)
            ? $data['probeUrl']
            : null;
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
            'joinMode' => $this->joinMode,
            'qualityScore' => $this->qualityScore,
            'desiredConfigRevision' => $this->desiredConfigRevision,
            'appliedConfigRevision' => $this->appliedConfigRevision,
            'desiredApplicationsRevision' => $this->desiredApplicationsRevision,
            'appliedApplicationsRevision' => $this->appliedApplicationsRevision,
            'syncStatus' => $this->syncStatus,
            'routingEnabled' => $this->routingEnabled,
            'draining' => $this->draining,
            'ejected' => $this->ejected,
            'restartCount' => $this->restartCount,
            'labels' => array_map(static fn($item) => $item, $this->labels),
            'routingWeight' => $this->routingWeight,
            'maintenanceNote' => $this->maintenanceNote,
            'probeFailures' => $this->probeFailures,
            'probeUrl' => $this->probeUrl,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
