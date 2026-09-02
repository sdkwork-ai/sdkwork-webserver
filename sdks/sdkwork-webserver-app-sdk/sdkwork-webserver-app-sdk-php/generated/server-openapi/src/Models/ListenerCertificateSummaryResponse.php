<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

use SDKWork\Web\AppSdk\Models\CertificateIdentifierResponse;

final class ListenerCertificateSummaryResponse
{
    public ?string $certName = null;

    public array $identifiers = [];

    public ?string $issuer = null;

    public ?string $fingerprint = null;

    public ?string $notAfter = null;

    public ?string $status = null;

    public function __construct(array $data = [])
    {
        $this->certName = array_key_exists('certName', $data)
            ? $data['certName']
            : null;
        $this->identifiers = array_key_exists('identifiers', $data)
            ? is_array($data['identifiers'])
                ? array_values(array_map(static fn($item) => is_array($item) ? CertificateIdentifierResponse::fromArray($item) : $item, $data['identifiers']))
                : []
            : [];
        $this->issuer = array_key_exists('issuer', $data)
            ? $data['issuer']
            : null;
        $this->fingerprint = array_key_exists('fingerprint', $data)
            ? $data['fingerprint']
            : null;
        $this->notAfter = array_key_exists('notAfter', $data)
            ? $data['notAfter']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'certName' => $this->certName,
            'identifiers' => array_values(array_map(static fn($item) => $item instanceof CertificateIdentifierResponse ? $item->toArray() : $item, $this->identifiers)),
            'issuer' => $this->issuer,
            'fingerprint' => $this->fingerprint,
            'notAfter' => $this->notAfter,
            'status' => $this->status,
        ];
    }
}
