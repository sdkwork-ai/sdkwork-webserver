<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class NginxReloadResponse
{
    public ?bool $success = null;

    public ?string $message = null;

    public ?string $timestamp = null;

    public function __construct(array $data = [])
    {
        $this->success = array_key_exists('success', $data)
            ? $data['success']
            : null;
        $this->message = array_key_exists('message', $data)
            ? $data['message']
            : null;
        $this->timestamp = array_key_exists('timestamp', $data)
            ? $data['timestamp']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'success' => $this->success,
            'message' => $this->message,
            'timestamp' => $this->timestamp,
        ];
    }
}
