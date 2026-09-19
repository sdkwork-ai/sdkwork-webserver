<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterHeartbeatSampleResponse
{
    public ?string $id = null;

    public ?int $status = null;

    public ?int $latencyMs = null;

    /** Resource metrics snapshot captured at heartbeat time. */
    public array $metrics = [];

    public ?string $reportedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->latencyMs = array_key_exists('latencyMs', $data)
            ? $data['latencyMs']
            : null;
        $this->metrics = array_key_exists('metrics', $data)
            ? is_array($data['metrics']) ? $data['metrics'] : []
            : [];
        $this->reportedAt = array_key_exists('reportedAt', $data)
            ? $data['reportedAt']
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
            'status' => $this->status,
            'latencyMs' => $this->latencyMs,
            'metrics' => $this->metrics,
            'reportedAt' => $this->reportedAt,
        ];
    }
}
