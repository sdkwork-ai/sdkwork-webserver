<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CreateListenerCertificateBindingRequest
{
    public ?string $certificateId = null;

    /** Immutable certificate version. Omit to bind the certificate's current active version. */
    public ?string $certificateVersionId = null;

    public ?int $priority = null;

    public ?bool $isDefault = null;

    public function __construct(array $data = [])
    {
        $this->certificateId = array_key_exists('certificateId', $data)
            ? $data['certificateId']
            : null;
        $this->certificateVersionId = array_key_exists('certificateVersionId', $data)
            ? $data['certificateVersionId']
            : null;
        $this->priority = array_key_exists('priority', $data)
            ? $data['priority']
            : null;
        $this->isDefault = array_key_exists('isDefault', $data)
            ? $data['isDefault']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'certificateId' => $this->certificateId,
            'certificateVersionId' => $this->certificateVersionId,
            'priority' => $this->priority,
            'isDefault' => $this->isDefault,
        ];
    }
}
