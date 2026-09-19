<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class EnqueueClusterPeerMessagesResponse
{
    public ?string $enqueued = null;

    public function __construct(array $data = [])
    {
        $this->enqueued = array_key_exists('enqueued', $data)
            ? $data['enqueued']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'enqueued' => $this->enqueued,
        ];
    }
}
