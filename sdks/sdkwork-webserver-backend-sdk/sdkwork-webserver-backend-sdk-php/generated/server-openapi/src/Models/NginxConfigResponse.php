<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class NginxConfigResponse
{
    public ?string $id = null;

    public ?int $configType = null;

    public ?string $configName = null;

    public ?string $configContent = null;

    public ?string $configHash = null;

    public ?bool $isActive = null;

    public ?int $versionNo = null;

    public ?string $deployedAt = null;

    public ?int $status = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->configType = array_key_exists('configType', $data)
            ? $data['configType']
            : null;
        $this->configName = array_key_exists('configName', $data)
            ? $data['configName']
            : null;
        $this->configContent = array_key_exists('configContent', $data)
            ? $data['configContent']
            : null;
        $this->configHash = array_key_exists('configHash', $data)
            ? $data['configHash']
            : null;
        $this->isActive = array_key_exists('isActive', $data)
            ? $data['isActive']
            : null;
        $this->versionNo = array_key_exists('versionNo', $data)
            ? $data['versionNo']
            : null;
        $this->deployedAt = array_key_exists('deployedAt', $data)
            ? $data['deployedAt']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
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
            'configType' => $this->configType,
            'configName' => $this->configName,
            'configContent' => $this->configContent,
            'configHash' => $this->configHash,
            'isActive' => $this->isActive,
            'versionNo' => $this->versionNo,
            'deployedAt' => $this->deployedAt,
            'status' => $this->status,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
