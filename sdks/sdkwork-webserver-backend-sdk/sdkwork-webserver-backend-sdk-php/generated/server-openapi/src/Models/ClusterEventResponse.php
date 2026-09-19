<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ClusterEventResponse
{
    public ?string $id = null;

    public ?string $clusterId = null;

    public ?string $hostId = null;

    public ?string $instanceId = null;

    public ?string $eventType = null;

    public ?string $severity = null;

    public ?string $message = null;

    public array $detail = [];

    public ?string $occurredAt = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->clusterId = array_key_exists('clusterId', $data)
            ? $data['clusterId']
            : null;
        $this->hostId = array_key_exists('hostId', $data)
            ? $data['hostId']
            : null;
        $this->instanceId = array_key_exists('instanceId', $data)
            ? $data['instanceId']
            : null;
        $this->eventType = array_key_exists('eventType', $data)
            ? $data['eventType']
            : null;
        $this->severity = array_key_exists('severity', $data)
            ? $data['severity']
            : null;
        $this->message = array_key_exists('message', $data)
            ? $data['message']
            : null;
        $this->detail = array_key_exists('detail', $data)
            ? is_array($data['detail']) ? $data['detail'] : []
            : [];
        $this->occurredAt = array_key_exists('occurredAt', $data)
            ? $data['occurredAt']
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
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
            'clusterId' => $this->clusterId,
            'hostId' => $this->hostId,
            'instanceId' => $this->instanceId,
            'eventType' => $this->eventType,
            'severity' => $this->severity,
            'message' => $this->message,
            'detail' => $this->detail,
            'occurredAt' => $this->occurredAt,
            'createdAt' => $this->createdAt,
        ];
    }
}
