<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class AgentCertificateObservation
{
    public ?string $certificateId = null;

    public ?string $fingerprint = null;

    public ?string $syncVersion = null;

    public ?string $state = null;

    public ?string $observedAt = null;

    public ?string $failureCode = null;

    public function __construct(array $data = [])
    {
        $this->certificateId = array_key_exists('certificateId', $data)
            ? $data['certificateId']
            : null;
        $this->fingerprint = array_key_exists('fingerprint', $data)
            ? $data['fingerprint']
            : null;
        $this->syncVersion = array_key_exists('syncVersion', $data)
            ? $data['syncVersion']
            : null;
        $this->state = array_key_exists('state', $data)
            ? $data['state']
            : null;
        $this->observedAt = array_key_exists('observedAt', $data)
            ? $data['observedAt']
            : null;
        $this->failureCode = array_key_exists('failureCode', $data)
            ? $data['failureCode']
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
            'fingerprint' => $this->fingerprint,
            'syncVersion' => $this->syncVersion,
            'state' => $this->state,
            'observedAt' => $this->observedAt,
            'failureCode' => $this->failureCode,
        ];
    }
}
