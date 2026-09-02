<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class NginxStatusResponse
{
    public ?bool $running = null;

    public ?string $version = null;

    public ?int $pid = null;

    public ?int $activeConnections = null;

    public ?string $configPath = null;

    public ?string $uptime = null;

    public function __construct(array $data = [])
    {
        $this->running = array_key_exists('running', $data)
            ? $data['running']
            : null;
        $this->version = array_key_exists('version', $data)
            ? $data['version']
            : null;
        $this->pid = array_key_exists('pid', $data)
            ? $data['pid']
            : null;
        $this->activeConnections = array_key_exists('activeConnections', $data)
            ? $data['activeConnections']
            : null;
        $this->configPath = array_key_exists('configPath', $data)
            ? $data['configPath']
            : null;
        $this->uptime = array_key_exists('uptime', $data)
            ? $data['uptime']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'running' => $this->running,
            'version' => $this->version,
            'pid' => $this->pid,
            'activeConnections' => $this->activeConnections,
            'configPath' => $this->configPath,
            'uptime' => $this->uptime,
        ];
    }
}
