<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class TunnelMachineRouteDeclarationResponse
{
    public ?string $routeId = null;

    public ?bool $active = null;

    public ?string $publicUrl = null;

    public ?string $detail = null;

    public function __construct(array $data = [])
    {
        $this->routeId = array_key_exists('routeId', $data)
            ? $data['routeId']
            : null;
        $this->active = array_key_exists('active', $data)
            ? $data['active']
            : null;
        $this->publicUrl = array_key_exists('publicUrl', $data)
            ? $data['publicUrl']
            : null;
        $this->detail = array_key_exists('detail', $data)
            ? $data['detail']
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
            'active' => $this->active,
            'publicUrl' => $this->publicUrl,
            'detail' => $this->detail,
        ];
    }
}
