<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class CreateRootDomainRequest
{
    public ?string $hostname = null;

    /** Cloud account whose DNS automation this root domain is bound to. Omitted leaves the Zone resolving its account per operation, which is the state every root domain reconciled from the edge's own configuration is in. */
    public ?string $cloudAccountId = null;

    public function __construct(array $data = [])
    {
        $this->hostname = array_key_exists('hostname', $data)
            ? $data['hostname']
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
            'hostname' => $this->hostname,
            'cloudAccountId' => $this->cloudAccountId,
        ];
    }
}
