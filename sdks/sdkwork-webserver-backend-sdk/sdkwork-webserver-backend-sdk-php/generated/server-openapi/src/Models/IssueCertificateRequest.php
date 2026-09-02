<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class IssueCertificateRequest
{
    /** Ordered exact or wildcard domain identifiers included in the certificate SAN extension. */
    public array $domainIds = [];

    /** 1=Let's Encrypt, 3=self-signed. Custom import is a separate future workflow. */
    public ?int $certType = null;

    public ?string $keyAlgorithm = null;

    public ?bool $autoRenew = null;

    public function __construct(array $data = [])
    {
        $this->domainIds = array_key_exists('domainIds', $data)
            ? is_array($data['domainIds'])
                ? array_values(array_map(static fn($item) => $item, $data['domainIds']))
                : []
            : [];
        $this->certType = array_key_exists('certType', $data)
            ? $data['certType']
            : null;
        $this->keyAlgorithm = array_key_exists('keyAlgorithm', $data)
            ? $data['keyAlgorithm']
            : null;
        $this->autoRenew = array_key_exists('autoRenew', $data)
            ? $data['autoRenew']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'domainIds' => array_values(array_map(static fn($item) => $item, $this->domainIds)),
            'certType' => $this->certType,
            'keyAlgorithm' => $this->keyAlgorithm,
            'autoRenew' => $this->autoRenew,
        ];
    }
}
