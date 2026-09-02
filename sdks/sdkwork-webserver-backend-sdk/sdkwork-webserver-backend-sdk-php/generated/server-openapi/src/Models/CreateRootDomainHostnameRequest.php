<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CreateRootDomainHostnameRequest
{
    /** Relative hostname such as @, www, or api.internal. */
    public ?string $recordName = null;

    public ?string $applicationId = null;

    public ?bool $isPrimary = null;

    public ?bool $sslEnabled = null;

    public ?string $sslProvider = null;

    public function __construct(array $data = [])
    {
        $this->recordName = array_key_exists('recordName', $data)
            ? $data['recordName']
            : null;
        $this->applicationId = array_key_exists('applicationId', $data)
            ? $data['applicationId']
            : null;
        $this->isPrimary = array_key_exists('isPrimary', $data)
            ? $data['isPrimary']
            : null;
        $this->sslEnabled = array_key_exists('sslEnabled', $data)
            ? $data['sslEnabled']
            : null;
        $this->sslProvider = array_key_exists('sslProvider', $data)
            ? $data['sslProvider']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'recordName' => $this->recordName,
            'applicationId' => $this->applicationId,
            'isPrimary' => $this->isPrimary,
            'sslEnabled' => $this->sslEnabled,
            'sslProvider' => $this->sslProvider,
        ];
    }
}
