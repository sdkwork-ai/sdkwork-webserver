<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\TunnelMachineRouteResponse;

final class TunnelMachineResponse
{
    public ?string $deviceId = null;

    public ?string $deviceName = null;

    public ?string $platform = null;

    public ?bool $online = null;

    public ?bool $enabled = null;

    public ?string $sessionId = null;

    public ?string $firstSeenAt = null;

    public ?string $lastSeenAt = null;

    public ?string $connectedAt = null;

    public ?string $disconnectedAt = null;

    public ?string $remoteAddr = null;

    public ?string $bytesIn = null;

    public ?string $bytesOut = null;

    public ?string $sessionsTotal = null;

    public ?string $routeCount = null;

    public ?string $healthyRouteCount = null;

    public array $unhealthyRouteIds = [];

    public array $routes = [];

    public function __construct(array $data = [])
    {
        $this->deviceId = array_key_exists('deviceId', $data)
            ? $data['deviceId']
            : null;
        $this->deviceName = array_key_exists('deviceName', $data)
            ? $data['deviceName']
            : null;
        $this->platform = array_key_exists('platform', $data)
            ? $data['platform']
            : null;
        $this->online = array_key_exists('online', $data)
            ? $data['online']
            : null;
        $this->enabled = array_key_exists('enabled', $data)
            ? $data['enabled']
            : null;
        $this->sessionId = array_key_exists('sessionId', $data)
            ? $data['sessionId']
            : null;
        $this->firstSeenAt = array_key_exists('firstSeenAt', $data)
            ? $data['firstSeenAt']
            : null;
        $this->lastSeenAt = array_key_exists('lastSeenAt', $data)
            ? $data['lastSeenAt']
            : null;
        $this->connectedAt = array_key_exists('connectedAt', $data)
            ? $data['connectedAt']
            : null;
        $this->disconnectedAt = array_key_exists('disconnectedAt', $data)
            ? $data['disconnectedAt']
            : null;
        $this->remoteAddr = array_key_exists('remoteAddr', $data)
            ? $data['remoteAddr']
            : null;
        $this->bytesIn = array_key_exists('bytesIn', $data)
            ? $data['bytesIn']
            : null;
        $this->bytesOut = array_key_exists('bytesOut', $data)
            ? $data['bytesOut']
            : null;
        $this->sessionsTotal = array_key_exists('sessionsTotal', $data)
            ? $data['sessionsTotal']
            : null;
        $this->routeCount = array_key_exists('routeCount', $data)
            ? $data['routeCount']
            : null;
        $this->healthyRouteCount = array_key_exists('healthyRouteCount', $data)
            ? $data['healthyRouteCount']
            : null;
        $this->unhealthyRouteIds = array_key_exists('unhealthyRouteIds', $data)
            ? is_array($data['unhealthyRouteIds'])
                ? array_values(array_map(static fn($item) => $item, $data['unhealthyRouteIds']))
                : []
            : [];
        $this->routes = array_key_exists('routes', $data)
            ? is_array($data['routes'])
                ? array_values(array_map(static fn($item) => is_array($item) ? TunnelMachineRouteResponse::fromArray($item) : $item, $data['routes']))
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
            'deviceId' => $this->deviceId,
            'deviceName' => $this->deviceName,
            'platform' => $this->platform,
            'online' => $this->online,
            'enabled' => $this->enabled,
            'sessionId' => $this->sessionId,
            'firstSeenAt' => $this->firstSeenAt,
            'lastSeenAt' => $this->lastSeenAt,
            'connectedAt' => $this->connectedAt,
            'disconnectedAt' => $this->disconnectedAt,
            'remoteAddr' => $this->remoteAddr,
            'bytesIn' => $this->bytesIn,
            'bytesOut' => $this->bytesOut,
            'sessionsTotal' => $this->sessionsTotal,
            'routeCount' => $this->routeCount,
            'healthyRouteCount' => $this->healthyRouteCount,
            'unhealthyRouteIds' => array_values(array_map(static fn($item) => $item, $this->unhealthyRouteIds)),
            'routes' => array_values(array_map(static fn($item) => $item instanceof TunnelMachineRouteResponse ? $item->toArray() : $item, $this->routes)),
        ];
    }
}
