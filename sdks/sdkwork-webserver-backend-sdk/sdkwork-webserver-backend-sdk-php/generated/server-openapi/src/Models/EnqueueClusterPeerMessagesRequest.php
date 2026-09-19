<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class EnqueueClusterPeerMessagesRequest
{
    public ?string $clusterId = null;

    /** Target instance; omitted broadcasts to every online member. */
    public ?string $toInstanceId = null;

    /** Sender instance; omitted for control-plane-originated messages. */
    public ?string $fromInstanceId = null;

    public ?string $messageType = null;

    /** Message payload JSON; bounded to 16 KiB. */
    public array $payload = [];

    public ?int $expiresInSeconds = null;

    public function __construct(array $data = [])
    {
        $this->clusterId = array_key_exists('clusterId', $data)
            ? $data['clusterId']
            : null;
        $this->toInstanceId = array_key_exists('toInstanceId', $data)
            ? $data['toInstanceId']
            : null;
        $this->fromInstanceId = array_key_exists('fromInstanceId', $data)
            ? $data['fromInstanceId']
            : null;
        $this->messageType = array_key_exists('messageType', $data)
            ? $data['messageType']
            : null;
        $this->payload = array_key_exists('payload', $data)
            ? is_array($data['payload']) ? $data['payload'] : []
            : [];
        $this->expiresInSeconds = array_key_exists('expiresInSeconds', $data)
            ? $data['expiresInSeconds']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'clusterId' => $this->clusterId,
            'toInstanceId' => $this->toInstanceId,
            'fromInstanceId' => $this->fromInstanceId,
            'messageType' => $this->messageType,
            'payload' => $this->payload,
            'expiresInSeconds' => $this->expiresInSeconds,
        ];
    }
}
