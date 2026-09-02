<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class CertificateOperationResponse
{
    public ?string $id = null;

    public ?string $certificateId = null;

    public ?string $operationType = null;

    public ?string $status = null;

    public ?int $attemptCount = null;

    public ?int $maxAttempts = null;

    public ?string $nextAttemptAt = null;

    public ?string $failureCode = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public ?string $completedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->certificateId = array_key_exists('certificateId', $data)
            ? $data['certificateId']
            : null;
        $this->operationType = array_key_exists('operationType', $data)
            ? $data['operationType']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->attemptCount = array_key_exists('attemptCount', $data)
            ? $data['attemptCount']
            : null;
        $this->maxAttempts = array_key_exists('maxAttempts', $data)
            ? $data['maxAttempts']
            : null;
        $this->nextAttemptAt = array_key_exists('nextAttemptAt', $data)
            ? $data['nextAttemptAt']
            : null;
        $this->failureCode = array_key_exists('failureCode', $data)
            ? $data['failureCode']
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
            : null;
        $this->updatedAt = array_key_exists('updatedAt', $data)
            ? $data['updatedAt']
            : null;
        $this->completedAt = array_key_exists('completedAt', $data)
            ? $data['completedAt']
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
            'certificateId' => $this->certificateId,
            'operationType' => $this->operationType,
            'status' => $this->status,
            'attemptCount' => $this->attemptCount,
            'maxAttempts' => $this->maxAttempts,
            'nextAttemptAt' => $this->nextAttemptAt,
            'failureCode' => $this->failureCode,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
            'completedAt' => $this->completedAt,
        ];
    }
}
