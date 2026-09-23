<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ProbeClusterInstanceRequest
{
    /** Probe path; defaults to `/`. */
    public ?string $path = null;

    /** Probe timeout, clamped to 100..=10000 ms. */
    public ?int $timeoutMs = null;

    public function __construct(array $data = [])
    {
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->timeoutMs = array_key_exists('timeoutMs', $data)
            ? $data['timeoutMs']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'path' => $this->path,
            'timeoutMs' => $this->timeoutMs,
        ];
    }
}
