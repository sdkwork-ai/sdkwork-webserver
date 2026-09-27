<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class DnsAccountResponse
{
    /** Stable operator-facing reference, and the value `providerAccountId` on an issue request names. */
    public ?string $accountId = null;

    /** Provider family whose API this account presents challenges through. */
    public ?string $provider = null;

    /** Hosted zone apex the account can publish into. An identifier is covered when it equals this apex or lives beneath it. */
    public ?string $zoneApex = null;

    public function __construct(array $data = [])
    {
        $this->accountId = array_key_exists('accountId', $data)
            ? $data['accountId']
            : null;
        $this->provider = array_key_exists('provider', $data)
            ? $data['provider']
            : null;
        $this->zoneApex = array_key_exists('zoneApex', $data)
            ? $data['zoneApex']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'accountId' => $this->accountId,
            'provider' => $this->provider,
            'zoneApex' => $this->zoneApex,
        ];
    }
}
