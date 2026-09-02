<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class HealthCheckResponse
{
    public ?string $id = null;

    public ?int $checkType = null;

    public ?string $checkUrl = null;

    public ?int $checkInterval = null;

    public ?int $timeoutMs = null;

    public ?int $retryCount = null;

    public ?int $status = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->checkType = array_key_exists('checkType', $data)
            ? $data['checkType']
            : null;
        $this->checkUrl = array_key_exists('checkUrl', $data)
            ? $data['checkUrl']
            : null;
        $this->checkInterval = array_key_exists('checkInterval', $data)
            ? $data['checkInterval']
            : null;
        $this->timeoutMs = array_key_exists('timeoutMs', $data)
            ? $data['timeoutMs']
            : null;
        $this->retryCount = array_key_exists('retryCount', $data)
            ? $data['retryCount']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
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
            'id' => $this->id,
            'checkType' => $this->checkType,
            'checkUrl' => $this->checkUrl,
            'checkInterval' => $this->checkInterval,
            'timeoutMs' => $this->timeoutMs,
            'retryCount' => $this->retryCount,
            'status' => $this->status,
            'createdAt' => $this->createdAt,
        ];
    }
}
