<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class DomainResponse
{
    public ?string $id = null;

    public ?string $hostname = null;

    public ?string $applicationId = null;

    public ?string $applicationName = null;

    public ?string $certificateCount = null;

    public ?bool $isPrimary = null;

    public ?bool $isVerified = null;

    public ?bool $sslEnabled = null;

    public ?string $sslProvider = null;

    public ?int $status = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
            : null;
        $this->applicationId = array_key_exists('applicationId', $data)
            ? $data['applicationId']
            : null;
        $this->applicationName = array_key_exists('applicationName', $data)
            ? $data['applicationName']
            : null;
        $this->certificateCount = array_key_exists('certificateCount', $data)
            ? $data['certificateCount']
            : null;
        $this->isPrimary = array_key_exists('isPrimary', $data)
            ? $data['isPrimary']
            : null;
        $this->isVerified = array_key_exists('isVerified', $data)
            ? $data['isVerified']
            : null;
        $this->sslEnabled = array_key_exists('sslEnabled', $data)
            ? $data['sslEnabled']
            : null;
        $this->sslProvider = array_key_exists('sslProvider', $data)
            ? $data['sslProvider']
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
            'hostname' => $this->hostname,
            'applicationId' => $this->applicationId,
            'applicationName' => $this->applicationName,
            'certificateCount' => $this->certificateCount,
            'isPrimary' => $this->isPrimary,
            'isVerified' => $this->isVerified,
            'sslEnabled' => $this->sslEnabled,
            'sslProvider' => $this->sslProvider,
            'status' => $this->status,
            'createdAt' => $this->createdAt,
        ];
    }
}
