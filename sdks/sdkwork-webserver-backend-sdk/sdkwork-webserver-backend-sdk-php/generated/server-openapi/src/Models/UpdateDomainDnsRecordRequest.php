<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class UpdateDomainDnsRecordRequest
{
    public ?string $recordType = null;

    public ?string $host = null;

    public ?string $recordValue = null;

    public ?int $ttlSeconds = null;

    public ?int $priority = null;

    public ?string $recordLine = null;

    public function __construct(array $data = [])
    {
        $this->recordType = array_key_exists('recordType', $data)
            ? $data['recordType']
            : null;
        $this->host = array_key_exists('host', $data)
            ? $data['host']
            : null;
        $this->recordValue = array_key_exists('recordValue', $data)
            ? $data['recordValue']
            : null;
        $this->ttlSeconds = array_key_exists('ttlSeconds', $data)
            ? $data['ttlSeconds']
            : null;
        $this->priority = array_key_exists('priority', $data)
            ? $data['priority']
            : null;
        $this->recordLine = array_key_exists('recordLine', $data)
            ? $data['recordLine']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'recordType' => $this->recordType,
            'host' => $this->host,
            'recordValue' => $this->recordValue,
            'ttlSeconds' => $this->ttlSeconds,
            'priority' => $this->priority,
            'recordLine' => $this->recordLine,
        ];
    }
}
