<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class RevokeCertificateRequest
{
    /** RFC 5280 section 5.3.1 revocation reason. */
    public ?string $reason = null;

    public function __construct(array $data = [])
    {
        $this->reason = array_key_exists('reason', $data)
            ? $data['reason']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'reason' => $this->reason,
        ];
    }
}
