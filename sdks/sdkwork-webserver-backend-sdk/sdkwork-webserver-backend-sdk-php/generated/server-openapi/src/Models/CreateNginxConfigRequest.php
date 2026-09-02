<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CreateNginxConfigRequest
{
    public ?int $configType = null;

    public ?string $configName = null;

    public ?string $configContent = null;

    public ?string $siteId = null;

    public function __construct(array $data = [])
    {
        $this->configType = array_key_exists('configType', $data)
            ? $data['configType']
            : null;
        $this->configName = array_key_exists('configName', $data)
            ? $data['configName']
            : null;
        $this->configContent = array_key_exists('configContent', $data)
            ? $data['configContent']
            : null;
        $this->siteId = array_key_exists('siteId', $data)
            ? $data['siteId']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'configType' => $this->configType,
            'configName' => $this->configName,
            'configContent' => $this->configContent,
            'siteId' => $this->siteId,
        ];
    }
}
