<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class ApplicationSourceVersionConfigSnapshot
{
    public ?string $appConfigPath = null;

    public ?string $deploymentConfigPath = null;

    public ?bool $appConfigDetected = null;

    public ?bool $deploymentConfigDetected = null;

    public function __construct(array $data = [])
    {
        $this->appConfigPath = array_key_exists('appConfigPath', $data)
            ? $data['appConfigPath']
            : null;
        $this->deploymentConfigPath = array_key_exists('deploymentConfigPath', $data)
            ? $data['deploymentConfigPath']
            : null;
        $this->appConfigDetected = array_key_exists('appConfigDetected', $data)
            ? $data['appConfigDetected']
            : null;
        $this->deploymentConfigDetected = array_key_exists('deploymentConfigDetected', $data)
            ? $data['deploymentConfigDetected']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'appConfigPath' => $this->appConfigPath,
            'deploymentConfigPath' => $this->deploymentConfigPath,
            'appConfigDetected' => $this->appConfigDetected,
            'deploymentConfigDetected' => $this->deploymentConfigDetected,
        ];
    }
}
