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
        ];
    }
}
