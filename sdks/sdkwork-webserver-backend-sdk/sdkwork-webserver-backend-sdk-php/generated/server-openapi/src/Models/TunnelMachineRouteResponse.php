<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class TunnelMachineRouteResponse
{
    public ?string $routeId = null;

    public ?string $name = null;

    public ?string $protocol = null;

    public ?string $domain = null;

    public ?int $port = null;

    public ?string $secretName = null;

    public array $locations = [];

    public ?string $loadBalancerGroup = null;

    public ?string $bandwidthLimitBytesPerSecond = null;

    public ?string $proxyProtocol = null;

    public ?bool $allowPublic = null;

    public ?string $auth = null;

    public array $allowedIps = [];

    public ?bool $healthy = null;

    public function __construct(array $data = [])
    {
        $this->routeId = array_key_exists('routeId', $data)
            ? $data['routeId']
            : null;
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->protocol = array_key_exists('protocol', $data)
            ? $data['protocol']
            : null;
        $this->domain = array_key_exists('domain', $data)
            ? $data['domain']
            : null;
        $this->port = array_key_exists('port', $data)
            ? $data['port']
            : null;
        $this->secretName = array_key_exists('secretName', $data)
            ? $data['secretName']
            : null;
        $this->locations = array_key_exists('locations', $data)
            ? is_array($data['locations'])
                ? array_values(array_map(static fn($item) => $item, $data['locations']))
                : []
            : [];
        $this->loadBalancerGroup = array_key_exists('loadBalancerGroup', $data)
            ? $data['loadBalancerGroup']
            : null;
        $this->bandwidthLimitBytesPerSecond = array_key_exists('bandwidthLimitBytesPerSecond', $data)
            ? $data['bandwidthLimitBytesPerSecond']
            : null;
        $this->proxyProtocol = array_key_exists('proxyProtocol', $data)
            ? $data['proxyProtocol']
            : null;
        $this->allowPublic = array_key_exists('allowPublic', $data)
            ? $data['allowPublic']
            : null;
        $this->auth = array_key_exists('auth', $data)
            ? $data['auth']
            : null;
        $this->allowedIps = array_key_exists('allowedIps', $data)
            ? is_array($data['allowedIps'])
                ? array_values(array_map(static fn($item) => $item, $data['allowedIps']))
                : []
            : [];
        $this->healthy = array_key_exists('healthy', $data)
            ? $data['healthy']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'routeId' => $this->routeId,
            'name' => $this->name,
            'protocol' => $this->protocol,
            'domain' => $this->domain,
            'port' => $this->port,
            'secretName' => $this->secretName,
            'locations' => array_values(array_map(static fn($item) => $item, $this->locations)),
            'loadBalancerGroup' => $this->loadBalancerGroup,
            'bandwidthLimitBytesPerSecond' => $this->bandwidthLimitBytesPerSecond,
            'proxyProtocol' => $this->proxyProtocol,
            'allowPublic' => $this->allowPublic,
            'auth' => $this->auth,
            'allowedIps' => array_values(array_map(static fn($item) => $item, $this->allowedIps)),
            'healthy' => $this->healthy,
        ];
    }
}
