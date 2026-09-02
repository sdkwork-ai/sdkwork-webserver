<?php

declare(strict_types=1);

namespace SDKWork\Webserver\AppSdk\Models;

final class CreatePlatformTargetRequest
{
    public ?string $targetKey = null;

    public ?string $platform = null;

    public ?string $techStack = null;

    public array $architectures = [];

    public ?string $bundleId = null;

    public ?string $packageName = null;

    /** Platform application id (WeChat / Douyin mini program) */
    public ?string $appId = null;

    public ?string $bundleName = null;

    public array $allowedChannels = [];

    public function __construct(array $data = [])
    {
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
        $this->appId = array_key_exists('appId', $data)
            ? $data['appId']
            : null;
        $this->bundleName = array_key_exists('bundleName', $data)
            ? $data['bundleName']
            : null;
        $this->allowedChannels = array_key_exists('allowedChannels', $data)
            ? is_array($data['allowedChannels'])
                ? array_values(array_map(static fn($item) => $item, $data['allowedChannels']))
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
            'targetKey' => $this->targetKey,
            'platform' => $this->platform,
            'techStack' => $this->techStack,
            'architectures' => array_values(array_map(static fn($item) => $item, $this->architectures)),
            'bundleId' => $this->bundleId,
            'packageName' => $this->packageName,
            'appId' => $this->appId,
            'bundleName' => $this->bundleName,
            'allowedChannels' => array_values(array_map(static fn($item) => $item, $this->allowedChannels)),
        ];
    }
}
