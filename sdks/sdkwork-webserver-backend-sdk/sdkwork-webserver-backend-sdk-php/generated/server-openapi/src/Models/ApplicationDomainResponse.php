<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\DomainDeploymentResponse;

final class ApplicationDomainResponse
{
    public ?string $id = null;

    public ?string $hostname = null;

    public ?string $rootDomainId = null;

    public ?string $recordName = null;

    public ?string $applicationId = null;

    public ?string $applicationName = null;

    public ?string $certificateCount = null;

    public ?bool $isPrimary = null;

    public ?bool $isVerified = null;

    public ?bool $sslEnabled = null;

    public ?string $sslProvider = null;

    public ?int $status = null;

    public ?DomainDeploymentResponse $latestDeployment = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
            : null;
        $this->rootDomainId = array_key_exists('rootDomainId', $data)
            ? $data['rootDomainId']
            : null;
        $this->recordName = array_key_exists('recordName', $data)
            ? $data['recordName']
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
        $this->latestDeployment = array_key_exists('latestDeployment', $data)
            ? is_array($data['latestDeployment']) ? DomainDeploymentResponse::fromArray($data['latestDeployment']) : null
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
            'hostname' => $this->hostname,
            'rootDomainId' => $this->rootDomainId,
            'recordName' => $this->recordName,
            'applicationId' => $this->applicationId,
            'applicationName' => $this->applicationName,
            'certificateCount' => $this->certificateCount,
            'isPrimary' => $this->isPrimary,
            'isVerified' => $this->isVerified,
            'sslEnabled' => $this->sslEnabled,
            'sslProvider' => $this->sslProvider,
            'status' => $this->status,
            'latestDeployment' => $this->latestDeployment instanceof DomainDeploymentResponse ? $this->latestDeployment->toArray() : $this->latestDeployment,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
