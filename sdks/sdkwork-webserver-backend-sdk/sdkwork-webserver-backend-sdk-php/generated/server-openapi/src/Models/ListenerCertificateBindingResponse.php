<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\ListenerCertificateSummaryResponse;

final class ListenerCertificateBindingResponse
{
    public ?string $id = null;

    public ?string $siteId = null;

    public ?string $domainId = null;

    public ?string $certificateId = null;

    public ?string $desiredCertificateVersionId = null;

    public ?string $currentCertificateVersionId = null;

    public ?ListenerCertificateSummaryResponse $desiredCertificate = null;

    public ?ListenerCertificateSummaryResponse $currentCertificate = null;

    public ?string $keyAlgorithm = null;

    public ?int $priority = null;

    public ?bool $isDefault = null;

    public ?string $status = null;

    public ?string $activatedAt = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->siteId = array_key_exists('siteId', $data)
            ? $data['siteId']
            : null;
        $this->domainId = array_key_exists('domainId', $data)
            ? $data['domainId']
            : null;
        $this->certificateId = array_key_exists('certificateId', $data)
            ? $data['certificateId']
            : null;
        $this->desiredCertificateVersionId = array_key_exists('desiredCertificateVersionId', $data)
            ? $data['desiredCertificateVersionId']
            : null;
        $this->currentCertificateVersionId = array_key_exists('currentCertificateVersionId', $data)
            ? $data['currentCertificateVersionId']
            : null;
        $this->desiredCertificate = array_key_exists('desiredCertificate', $data)
            ? is_array($data['desiredCertificate']) ? ListenerCertificateSummaryResponse::fromArray($data['desiredCertificate']) : null
            : null;
        $this->currentCertificate = array_key_exists('currentCertificate', $data)
            ? is_array($data['currentCertificate']) ? ListenerCertificateSummaryResponse::fromArray($data['currentCertificate']) : null
            : null;
        $this->keyAlgorithm = array_key_exists('keyAlgorithm', $data)
            ? $data['keyAlgorithm']
            : null;
        $this->priority = array_key_exists('priority', $data)
            ? $data['priority']
            : null;
        $this->isDefault = array_key_exists('isDefault', $data)
            ? $data['isDefault']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->activatedAt = array_key_exists('activatedAt', $data)
            ? $data['activatedAt']
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
            'siteId' => $this->siteId,
            'domainId' => $this->domainId,
            'certificateId' => $this->certificateId,
            'desiredCertificateVersionId' => $this->desiredCertificateVersionId,
            'currentCertificateVersionId' => $this->currentCertificateVersionId,
            'desiredCertificate' => $this->desiredCertificate instanceof ListenerCertificateSummaryResponse ? $this->desiredCertificate->toArray() : $this->desiredCertificate,
            'currentCertificate' => $this->currentCertificate instanceof ListenerCertificateSummaryResponse ? $this->currentCertificate->toArray() : $this->currentCertificate,
            'keyAlgorithm' => $this->keyAlgorithm,
            'priority' => $this->priority,
            'isDefault' => $this->isDefault,
            'status' => $this->status,
            'activatedAt' => $this->activatedAt,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
