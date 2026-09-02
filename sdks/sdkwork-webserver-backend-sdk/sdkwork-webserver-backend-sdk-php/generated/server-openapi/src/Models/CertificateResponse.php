<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\CertificateIdentifierResponse;

final class CertificateResponse
{
    public ?string $id = null;

    public ?string $certName = null;

    public array $identifiers = [];

    public ?int $certType = null;

    public ?string $issuer = null;

    public ?string $fingerprint = null;

    public ?string $keyAlgorithm = null;

    public ?string $notBefore = null;

    public ?string $notAfter = null;

    public ?bool $autoRenew = null;

    public ?string $renewalStatus = null;

    public ?string $status = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->certName = array_key_exists('certName', $data)
            ? $data['certName']
            : null;
        $this->identifiers = array_key_exists('identifiers', $data)
            ? is_array($data['identifiers'])
                ? array_values(array_map(static fn($item) => is_array($item) ? CertificateIdentifierResponse::fromArray($item) : $item, $data['identifiers']))
                : []
            : [];
        $this->certType = array_key_exists('certType', $data)
            ? $data['certType']
            : null;
        $this->issuer = array_key_exists('issuer', $data)
            ? $data['issuer']
            : null;
        $this->fingerprint = array_key_exists('fingerprint', $data)
            ? $data['fingerprint']
            : null;
        $this->keyAlgorithm = array_key_exists('keyAlgorithm', $data)
            ? $data['keyAlgorithm']
            : null;
        $this->notBefore = array_key_exists('notBefore', $data)
            ? $data['notBefore']
            : null;
        $this->notAfter = array_key_exists('notAfter', $data)
            ? $data['notAfter']
            : null;
        $this->autoRenew = array_key_exists('autoRenew', $data)
            ? $data['autoRenew']
            : null;
        $this->renewalStatus = array_key_exists('renewalStatus', $data)
            ? $data['renewalStatus']
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
            'certName' => $this->certName,
            'identifiers' => array_values(array_map(static fn($item) => $item instanceof CertificateIdentifierResponse ? $item->toArray() : $item, $this->identifiers)),
            'certType' => $this->certType,
            'issuer' => $this->issuer,
            'fingerprint' => $this->fingerprint,
            'keyAlgorithm' => $this->keyAlgorithm,
            'notBefore' => $this->notBefore,
            'notAfter' => $this->notAfter,
            'autoRenew' => $this->autoRenew,
            'renewalStatus' => $this->renewalStatus,
            'status' => $this->status,
            'createdAt' => $this->createdAt,
        ];
    }
}
