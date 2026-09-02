<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class SdkWorkCommandData
{
    public ?bool $accepted = null;

    public ?string $resourceId = null;

    public ?string $status = null;

    public function __construct(array $data = [])
    {
        $this->accepted = array_key_exists('accepted', $data)
            ? $data['accepted']
            : null;
        $this->resourceId = array_key_exists('resourceId', $data)
            ? $data['resourceId']
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
            'accepted' => $this->accepted,
            'resourceId' => $this->resourceId,
            'status' => $this->status,
        ];
    }
}
