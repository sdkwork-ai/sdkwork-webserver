<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class PlatformTargetResponse
{
    public ?string $id = null;

    public ?string $appId = null;

    public ?string $targetKey = null;

    public ?string $platform = null;

    public ?string $techStack = null;

    public array $architectures = [];

    public ?string $bundleId = null;

    public ?string $packageName = null;

    public ?string $appIdValue = null;

    public ?string $bundleName = null;

    public ?string $targetStatus = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->appId = array_key_exists('appId', $data)
            ? $data['appId']
            : null;
        $this->targetKey = array_key_exists('targetKey', $data)
            ? $data['targetKey']
            : null;
        $this->platform = array_key_exists('platform', $data)
            ? $data['platform']
            : null;
        $this->techStack = array_key_exists('techStack', $data)
            ? $data['techStack']
            : null;
        $this->architectures = array_key_exists('architectures', $data)
            ? is_array($data['architectures'])
                ? array_values(array_map(static fn($item) => $item, $data['architectures']))
                : []
            : [];
        $this->bundleId = array_key_exists('bundleId', $data)
            ? $data['bundleId']
            : null;
        $this->packageName = array_key_exists('packageName', $data)
            ? $data['packageName']
            : null;
        $this->appIdValue = array_key_exists('appIdValue', $data)
            ? $data['appIdValue']
            : null;
        $this->bundleName = array_key_exists('bundleName', $data)
            ? $data['bundleName']
            : null;
        $this->targetStatus = array_key_exists('targetStatus', $data)
            ? $data['targetStatus']
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
            'appId' => $this->appId,
            'targetKey' => $this->targetKey,
            'platform' => $this->platform,
            'techStack' => $this->techStack,
            'architectures' => array_values(array_map(static fn($item) => $item, $this->architectures)),
            'bundleId' => $this->bundleId,
            'packageName' => $this->packageName,
            'appIdValue' => $this->appIdValue,
            'bundleName' => $this->bundleName,
            'targetStatus' => $this->targetStatus,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
