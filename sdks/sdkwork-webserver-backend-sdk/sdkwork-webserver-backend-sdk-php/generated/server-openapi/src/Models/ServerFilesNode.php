<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class ServerFilesNode
{
    public ?string $id = null;

    public ?string $name = null;

    public ?string $host = null;

    public ?int $sshPort = null;

    public ?string $status = null;

    /** Authorized filesystem root the node may browse (e.g. /opt/deploy). */
    public ?string $filesystemRoot = null;

    public ?string $region = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->host = array_key_exists('host', $data)
            ? $data['host']
            : null;
        $this->sshPort = array_key_exists('sshPort', $data)
            ? $data['sshPort']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->filesystemRoot = array_key_exists('filesystemRoot', $data)
            ? $data['filesystemRoot']
            : null;
        $this->region = array_key_exists('region', $data)
            ? $data['region']
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
            'name' => $this->name,
            'host' => $this->host,
            'sshPort' => $this->sshPort,
            'status' => $this->status,
            'filesystemRoot' => $this->filesystemRoot,
            'region' => $this->region,
        ];
    }
}
