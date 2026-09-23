<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterProbeRunResponse
{
    /** Probe reached the instance and got a healthy answer. */
    public ?bool $healthy = null;

    /** Round-trip latency of the probe. */
    public ?int $latencyMs = null;

    /** Consecutive probe failures after this run; 0 when healthy. */
    public ?int $failures = null;

    /** Auto-eject transition happened on this run. */
    public ?bool $ejected = null;

    /** Auto-recovery transition happened on this run. */
    public ?bool $recovered = null;

    public function __construct(array $data = [])
    {
        $this->healthy = array_key_exists('healthy', $data)
            ? $data['healthy']
            : null;
        $this->latencyMs = array_key_exists('latencyMs', $data)
            ? $data['latencyMs']
            : null;
        $this->failures = array_key_exists('failures', $data)
            ? $data['failures']
            : null;
        $this->ejected = array_key_exists('ejected', $data)
            ? $data['ejected']
            : null;
        $this->recovered = array_key_exists('recovered', $data)
            ? $data['recovered']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'healthy' => $this->healthy,
            'latencyMs' => $this->latencyMs,
            'failures' => $this->failures,
            'ejected' => $this->ejected,
            'recovered' => $this->recovered,
        ];
    }
}
