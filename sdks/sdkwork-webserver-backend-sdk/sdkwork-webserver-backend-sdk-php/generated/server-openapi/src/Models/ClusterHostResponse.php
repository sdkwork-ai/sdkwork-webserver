<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterHostResponse
{
    public ?string $id = null;

    public ?string $clusterId = null;

    public ?string $name = null;

    public ?string $hostname = null;

    public ?string $machineCode = null;

    public ?string $osName = null;

    public ?string $osVersion = null;

    public ?string $kernelVersion = null;

    public ?string $arch = null;

    public ?string $cpuModel = null;

    public ?int $cpuCores = null;

    public ?string $memoryTotalMb = null;

    public ?string $remoteIp = null;

    public array $localIps = [];

    public array $macAddresses = [];

    public ?string $daemonVersion = null;

    /** 0=offline, 1=online, 2=deploying, 3=error, 4=maintenance */
    public ?int $status = null;

    public ?string $lastHeartbeatAt = null;

    public ?string $instanceCount = null;

    /** `LAN` = same-subnet host with shared-database or direct-API reachability; `TUNNEL` = API-only host reached through the reverse tunnel. */
    public ?string $joinMode = null;

    /** Tunnel route domain for `TUNNEL` hosts; absent on `LAN` hosts. */
    public ?string $tunnelRouteDomain = null;

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
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
            : null;
        $this->machineCode = array_key_exists('machineCode', $data)
            ? $data['machineCode']
            : null;
        $this->osName = array_key_exists('osName', $data)
            ? $data['osName']
            : null;
        $this->osVersion = array_key_exists('osVersion', $data)
            ? $data['osVersion']
            : null;
        $this->kernelVersion = array_key_exists('kernelVersion', $data)
            ? $data['kernelVersion']
            : null;
        $this->arch = array_key_exists('arch', $data)
            ? $data['arch']
            : null;
        $this->cpuModel = array_key_exists('cpuModel', $data)
            ? $data['cpuModel']
            : null;
        $this->cpuCores = array_key_exists('cpuCores', $data)
            ? $data['cpuCores']
            : null;
        $this->memoryTotalMb = array_key_exists('memoryTotalMb', $data)
            ? $data['memoryTotalMb']
            : null;
        $this->remoteIp = array_key_exists('remoteIp', $data)
            ? $data['remoteIp']
            : null;
        $this->localIps = array_key_exists('localIps', $data)
            ? is_array($data['localIps'])
                ? array_values(array_map(static fn($item) => $item, $data['localIps']))
                : []
            : [];
        $this->macAddresses = array_key_exists('macAddresses', $data)
            ? is_array($data['macAddresses'])
                ? array_values(array_map(static fn($item) => $item, $data['macAddresses']))
                : []
            : [];
        $this->daemonVersion = array_key_exists('daemonVersion', $data)
            ? $data['daemonVersion']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->lastHeartbeatAt = array_key_exists('lastHeartbeatAt', $data)
            ? $data['lastHeartbeatAt']
            : null;
        $this->instanceCount = array_key_exists('instanceCount', $data)
            ? $data['instanceCount']
            : null;
        $this->joinMode = array_key_exists('joinMode', $data)
            ? $data['joinMode']
            : null;
        $this->tunnelRouteDomain = array_key_exists('tunnelRouteDomain', $data)
            ? $data['tunnelRouteDomain']
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
            'name' => $this->name,
            'hostname' => $this->hostname,
            'machineCode' => $this->machineCode,
            'osName' => $this->osName,
            'osVersion' => $this->osVersion,
            'kernelVersion' => $this->kernelVersion,
            'arch' => $this->arch,
            'cpuModel' => $this->cpuModel,
            'cpuCores' => $this->cpuCores,
            'memoryTotalMb' => $this->memoryTotalMb,
            'remoteIp' => $this->remoteIp,
            'localIps' => array_values(array_map(static fn($item) => $item, $this->localIps)),
            'macAddresses' => array_values(array_map(static fn($item) => $item, $this->macAddresses)),
            'daemonVersion' => $this->daemonVersion,
            'status' => $this->status,
            'lastHeartbeatAt' => $this->lastHeartbeatAt,
            'instanceCount' => $this->instanceCount,
            'joinMode' => $this->joinMode,
            'tunnelRouteDomain' => $this->tunnelRouteDomain,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
