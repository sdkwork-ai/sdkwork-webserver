<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class SdkWorkAsyncData
{
    public ?bool $accepted = null;

    public ?string $operationId = null;

    public ?string $status = null;

    public ?string $pollUrl = null;

    public function __construct(array $data = [])
    {
        $this->accepted = array_key_exists('accepted', $data)
            ? $data['accepted']
            : null;
        $this->operationId = array_key_exists('operationId', $data)
            ? $data['operationId']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->pollUrl = array_key_exists('pollUrl', $data)
            ? $data['pollUrl']
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
            'operationId' => $this->operationId,
            'status' => $this->status,
            'pollUrl' => $this->pollUrl,
        ];
    }
}
