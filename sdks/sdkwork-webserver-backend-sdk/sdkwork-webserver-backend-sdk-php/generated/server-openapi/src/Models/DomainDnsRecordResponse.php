<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class DomainDnsRecordResponse
{
    public ?string $id = null;

    /** Absolute record owner inside the zone, e.g. `www.example.com`. */
    public ?string $recordName = null;

    /** Record type as the provider spells it (A, AAAA, CNAME, TXT, MX). */
    public ?string $recordType = null;

    /** Record content — the resolution IP for A/AAAA, the target for CNAME/MX. */
    public ?string $recordValue = null;

    public ?int $ttlSeconds = null;

    /** Priority for the record types that carry one (MX, SRV). */
    public ?int $priority = null;

    /** Provider resolution line, when the provider splits one owner per line. */
    public ?string $recordLine = null;

    /** The registered subdomain this record resolves; absent when its owner matches none. */
    public ?string $domainId = null;

    /** Provider family the snapshot was read from. */
    public ?string $dnsProvider = null;

    /** The cloud account the snapshot was read through. */
    public ?string $cloudAccountId = null;

    /** Provider-assigned record identity, when the provider returned one. */
    public ?string $providerRecordRef = null;

    /** When this row was read from the provider. */
    public ?string $syncedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->recordName = array_key_exists('recordName', $data)
            ? $data['recordName']
            : null;
        $this->recordType = array_key_exists('recordType', $data)
            ? $data['recordType']
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
        $this->domainId = array_key_exists('domainId', $data)
            ? $data['domainId']
            : null;
        $this->dnsProvider = array_key_exists('dnsProvider', $data)
            ? $data['dnsProvider']
            : null;
        $this->cloudAccountId = array_key_exists('cloudAccountId', $data)
            ? $data['cloudAccountId']
            : null;
        $this->providerRecordRef = array_key_exists('providerRecordRef', $data)
            ? $data['providerRecordRef']
            : null;
        $this->syncedAt = array_key_exists('syncedAt', $data)
            ? $data['syncedAt']
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
            'recordName' => $this->recordName,
            'recordType' => $this->recordType,
            'recordValue' => $this->recordValue,
            'ttlSeconds' => $this->ttlSeconds,
            'priority' => $this->priority,
            'recordLine' => $this->recordLine,
            'domainId' => $this->domainId,
            'dnsProvider' => $this->dnsProvider,
            'cloudAccountId' => $this->cloudAccountId,
            'providerRecordRef' => $this->providerRecordRef,
            'syncedAt' => $this->syncedAt,
        ];
    }
}
