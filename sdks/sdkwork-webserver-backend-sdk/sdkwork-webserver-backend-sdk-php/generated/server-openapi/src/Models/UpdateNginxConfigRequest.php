<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class UpdateNginxConfigRequest
{
    public ?string $configContent = null;

    public ?string $configName = null;

    public function __construct(array $data = [])
    {
        $this->configContent = array_key_exists('configContent', $data)
            ? $data['configContent']
            : null;
        $this->configName = array_key_exists('configName', $data)
            ? $data['configName']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'configContent' => $this->configContent,
            'configName' => $this->configName,
        ];
    }
}
