<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

/**
 * Partial edit; an omitted member leaves the stored value unchanged. The apex hostname is not editable.
 */
final class UpdateRootDomainRequest
{
    public ?string $displayName = null;

    public ?string $dnsProvider = null;

    public ?string $providerZoneRef = null;

    /** 0=pending, 1=active, 2=disabled. */
    public ?int $status = null;

    /** The cloud account to bind, or `null` to unbind. The **only** member of this request that distinguishes "leave it alone" from "clear it": an omitted member keeps the stored account and an explicit `null` removes it. This is the one place the surface's "a blank value is an omission" reading does not apply, because the member names an association rather than describing the row. */
    public ?string $cloudAccountId = null;

    public function __construct(array $data = [])
    {
        $this->displayName = array_key_exists('displayName', $data)
            ? $data['displayName']
            : null;
        $this->dnsProvider = array_key_exists('dnsProvider', $data)
            ? $data['dnsProvider']
            : null;
        $this->providerZoneRef = array_key_exists('providerZoneRef', $data)
            ? $data['providerZoneRef']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
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
            'displayName' => $this->displayName,
            'dnsProvider' => $this->dnsProvider,
            'providerZoneRef' => $this->providerZoneRef,
            'status' => $this->status,
            'cloudAccountId' => $this->cloudAccountId,
        ];
    }
}
