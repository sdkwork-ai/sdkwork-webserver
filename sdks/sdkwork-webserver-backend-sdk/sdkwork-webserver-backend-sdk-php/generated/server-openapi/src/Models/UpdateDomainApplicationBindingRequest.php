<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class UpdateDomainApplicationBindingRequest
{
    public ?string $applicationId = null;

    public ?bool $isPrimary = null;

    public function __construct(array $data = [])
    {
        $this->applicationId = array_key_exists('applicationId', $data)
            ? $data['applicationId']
            : null;
        $this->isPrimary = array_key_exists('isPrimary', $data)
            ? $data['isPrimary']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'applicationId' => $this->applicationId,
            'isPrimary' => $this->isPrimary,
        ];
    }
}
