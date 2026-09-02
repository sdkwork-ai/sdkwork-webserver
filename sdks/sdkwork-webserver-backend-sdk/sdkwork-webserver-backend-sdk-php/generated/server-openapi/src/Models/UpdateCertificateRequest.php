<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class UpdateCertificateRequest
{
    public ?bool $autoRenew = null;

    public function __construct(array $data = [])
    {
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
            'autoRenew' => $this->autoRenew,
        ];
    }
}
