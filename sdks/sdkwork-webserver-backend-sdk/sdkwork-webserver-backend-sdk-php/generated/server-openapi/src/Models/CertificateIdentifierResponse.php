<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CertificateIdentifierResponse
{
    public ?string $domainId = null;

    public ?string $hostname = null;

    public ?string $identifierType = null;

    public ?int $position = null;

    public function __construct(array $data = [])
    {
        $this->domainId = array_key_exists('domainId', $data)
            ? $data['domainId']
            : null;
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
            : null;
        $this->identifierType = array_key_exists('identifierType', $data)
            ? $data['identifierType']
            : null;
        $this->position = array_key_exists('position', $data)
            ? $data['position']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'domainId' => $this->domainId,
            'hostname' => $this->hostname,
            'identifierType' => $this->identifierType,
            'position' => $this->position,
        ];
    }
}
