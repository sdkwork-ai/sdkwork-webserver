<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class NginxConfigRevisionResponse
{
    public ?int $versionNo = null;

    public ?string $configName = null;

    public ?string $configContent = null;

    public ?string $configHash = null;

    public ?string $createdBy = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->versionNo = array_key_exists('versionNo', $data)
            ? $data['versionNo']
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
        $this->createdBy = array_key_exists('createdBy', $data)
            ? $data['createdBy']
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'versionNo' => $this->versionNo,
            'configName' => $this->configName,
            'configContent' => $this->configContent,
            'configHash' => $this->configHash,
            'createdBy' => $this->createdBy,
            'createdAt' => $this->createdAt,
        ];
    }
}
