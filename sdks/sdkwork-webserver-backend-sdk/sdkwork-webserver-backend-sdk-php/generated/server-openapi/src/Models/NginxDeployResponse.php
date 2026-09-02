<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class NginxDeployResponse
{
    public ?bool $success = null;

    public ?string $configId = null;

    public ?string $deployedAt = null;

    public array $reloadResult = [];

    public function __construct(array $data = [])
    {
        $this->success = array_key_exists('success', $data)
            ? $data['success']
            : null;
        $this->configId = array_key_exists('configId', $data)
            ? $data['configId']
            : null;
        $this->deployedAt = array_key_exists('deployedAt', $data)
            ? $data['deployedAt']
            : null;
        $this->reloadResult = array_key_exists('reloadResult', $data)
            ? is_array($data['reloadResult']) ? $data['reloadResult'] : []
            : [];
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'success' => $this->success,
            'configId' => $this->configId,
            'deployedAt' => $this->deployedAt,
            'reloadResult' => $this->reloadResult,
        ];
    }
}
