<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class AuditLogResponse
{
    public ?string $id = null;

    /** Operator user id as a string to avoid JavaScript precision loss. */
    public ?string $operatorId = null;

    public ?string $operatorType = null;

    public ?string $action = null;

    public ?string $targetType = null;

    /** Target snowflake id as a string to avoid JavaScript precision loss. Null when the audit action has no specific numeric target. */
    public ?string $targetId = null;

    public ?string $targetUuid = null;

    public ?string $ipAddress = null;

    public array $changes = [];

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->operatorId = array_key_exists('operatorId', $data)
            ? $data['operatorId']
            : null;
        $this->operatorType = array_key_exists('operatorType', $data)
            ? $data['operatorType']
            : null;
        $this->action = array_key_exists('action', $data)
            ? $data['action']
            : null;
        $this->targetType = array_key_exists('targetType', $data)
            ? $data['targetType']
            : null;
        $this->targetId = array_key_exists('targetId', $data)
            ? $data['targetId']
            : null;
        $this->targetUuid = array_key_exists('targetUuid', $data)
            ? $data['targetUuid']
            : null;
        $this->ipAddress = array_key_exists('ipAddress', $data)
            ? $data['ipAddress']
            : null;
        $this->changes = array_key_exists('changes', $data)
            ? is_array($data['changes']) ? $data['changes'] : []
            : [];
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
            'operatorId' => $this->operatorId,
            'operatorType' => $this->operatorType,
            'action' => $this->action,
            'targetType' => $this->targetType,
            'targetId' => $this->targetId,
            'targetUuid' => $this->targetUuid,
            'ipAddress' => $this->ipAddress,
            'changes' => $this->changes,
            'createdAt' => $this->createdAt,
        ];
    }
}
