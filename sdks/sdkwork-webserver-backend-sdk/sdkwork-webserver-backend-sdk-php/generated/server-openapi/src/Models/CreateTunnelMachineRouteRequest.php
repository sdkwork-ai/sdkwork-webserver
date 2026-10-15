<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class CreateTunnelMachineRouteRequest
{
    public ?string $name = null;

    public ?string $protocol = null;

    public ?string $domain = null;

    public ?int $port = null;

    public ?bool $allowPublic = null;

    public array $allowedIps = [];

    public ?string $auth = null;

    public array $visitorTokens = [];

    public array $locations = [];

    public ?string $bandwidthLimitBytesPerSecond = null;

    public ?string $proxyProtocol = null;

    public ?string $loadBalancerGroup = null;

    public function __construct(array $data = [])
    {
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
        $this->allowPublic = array_key_exists('allowPublic', $data)
            ? $data['allowPublic']
            : null;
        $this->allowedIps = array_key_exists('allowedIps', $data)
            ? is_array($data['allowedIps'])
                ? array_values(array_map(static fn($item) => $item, $data['allowedIps']))
                : []
            : [];
        $this->auth = array_key_exists('auth', $data)
            ? $data['auth']
            : null;
        $this->visitorTokens = array_key_exists('visitorTokens', $data)
            ? is_array($data['visitorTokens'])
                ? array_values(array_map(static fn($item) => $item, $data['visitorTokens']))
                : []
            : [];
        $this->locations = array_key_exists('locations', $data)
            ? is_array($data['locations'])
                ? array_values(array_map(static fn($item) => $item, $data['locations']))
                : []
            : [];
        $this->bandwidthLimitBytesPerSecond = array_key_exists('bandwidthLimitBytesPerSecond', $data)
            ? $data['bandwidthLimitBytesPerSecond']
            : null;
        $this->proxyProtocol = array_key_exists('proxyProtocol', $data)
            ? $data['proxyProtocol']
            : null;
        $this->loadBalancerGroup = array_key_exists('loadBalancerGroup', $data)
            ? $data['loadBalancerGroup']
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
            'protocol' => $this->protocol,
            'domain' => $this->domain,
            'port' => $this->port,
            'allowPublic' => $this->allowPublic,
            'allowedIps' => array_values(array_map(static fn($item) => $item, $this->allowedIps)),
            'auth' => $this->auth,
            'visitorTokens' => array_values(array_map(static fn($item) => $item, $this->visitorTokens)),
            'locations' => array_values(array_map(static fn($item) => $item, $this->locations)),
            'bandwidthLimitBytesPerSecond' => $this->bandwidthLimitBytesPerSecond,
            'proxyProtocol' => $this->proxyProtocol,
            'loadBalancerGroup' => $this->loadBalancerGroup,
        ];
    }
}
