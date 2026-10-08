<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class DomainDnsSyncResponse
{
    public ?string $recordCount = null;

    public ?string $syncedAt = null;

    /** The zone apex the provider inventory was read for. */
    public ?string $zoneApex = null;

    public ?string $dnsProvider = null;

    public ?string $cloudAccountId = null;

    public function __construct(array $data = [])
    {
        $this->recordCount = array_key_exists('recordCount', $data)
            ? $data['recordCount']
            : null;
        $this->syncedAt = array_key_exists('syncedAt', $data)
            ? $data['syncedAt']
            : null;
        $this->zoneApex = array_key_exists('zoneApex', $data)
            ? $data['zoneApex']
            : null;
        $this->dnsProvider = array_key_exists('dnsProvider', $data)
            ? $data['dnsProvider']
            : null;
        $this->cloudAccountId = array_key_exists('cloudAccountId', $data)
            ? $data['cloudAccountId']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'recordCount' => $this->recordCount,
            'syncedAt' => $this->syncedAt,
            'zoneApex' => $this->zoneApex,
            'dnsProvider' => $this->dnsProvider,
            'cloudAccountId' => $this->cloudAccountId,
        ];
    }
}
