<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class DomainDnsRecordStatusRequest
{
    /** `true` resumes the record; `false` pauses it (暂停解析). */
    public ?bool $enabled = null;

    public function __construct(array $data = [])
    {
        $this->enabled = array_key_exists('enabled', $data)
            ? $data['enabled']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'enabled' => $this->enabled,
        ];
    }
}
