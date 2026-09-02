<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class RootDomainResponse
{
    public ?string $id = null;

    public ?string $hostname = null;

    public ?int $status = null;

    public ?string $subdomainCount = null;

    public ?string $boundSubdomainCount = null;

    public ?string $verifiedSubdomainCount = null;

    public ?string $httpsSubdomainCount = null;

    public ?string $activeDeploymentCount = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->subdomainCount = array_key_exists('subdomainCount', $data)
            ? $data['subdomainCount']
            : null;
        $this->boundSubdomainCount = array_key_exists('boundSubdomainCount', $data)
            ? $data['boundSubdomainCount']
            : null;
        $this->verifiedSubdomainCount = array_key_exists('verifiedSubdomainCount', $data)
            ? $data['verifiedSubdomainCount']
            : null;
        $this->httpsSubdomainCount = array_key_exists('httpsSubdomainCount', $data)
            ? $data['httpsSubdomainCount']
            : null;
        $this->activeDeploymentCount = array_key_exists('activeDeploymentCount', $data)
            ? $data['activeDeploymentCount']
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
            'hostname' => $this->hostname,
            'status' => $this->status,
            'subdomainCount' => $this->subdomainCount,
            'boundSubdomainCount' => $this->boundSubdomainCount,
            'verifiedSubdomainCount' => $this->verifiedSubdomainCount,
            'httpsSubdomainCount' => $this->httpsSubdomainCount,
            'activeDeploymentCount' => $this->activeDeploymentCount,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
