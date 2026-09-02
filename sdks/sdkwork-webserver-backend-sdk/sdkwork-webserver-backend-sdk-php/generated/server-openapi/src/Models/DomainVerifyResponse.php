<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class DomainVerifyResponse
{
    public ?bool $verified = null;

    public ?string $status = null;

    public ?string $method = null;

    public ?string $recordName = null;

    public ?string $recordValue = null;

    public ?int $attemptCount = null;

    public ?string $expiresAt = null;

    public ?string $nextAttemptAt = null;

    public ?string $checkedAt = null;

    public ?string $failureCode = null;

    public function __construct(array $data = [])
    {
        $this->verified = array_key_exists('verified', $data)
            ? $data['verified']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->method = array_key_exists('method', $data)
            ? $data['method']
            : null;
        $this->recordName = array_key_exists('recordName', $data)
            ? $data['recordName']
            : null;
        $this->recordValue = array_key_exists('recordValue', $data)
            ? $data['recordValue']
            : null;
        $this->attemptCount = array_key_exists('attemptCount', $data)
            ? $data['attemptCount']
            : null;
        $this->expiresAt = array_key_exists('expiresAt', $data)
            ? $data['expiresAt']
            : null;
        $this->nextAttemptAt = array_key_exists('nextAttemptAt', $data)
            ? $data['nextAttemptAt']
            : null;
        $this->checkedAt = array_key_exists('checkedAt', $data)
            ? $data['checkedAt']
            : null;
        $this->failureCode = array_key_exists('failureCode', $data)
            ? $data['failureCode']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'verified' => $this->verified,
            'status' => $this->status,
            'method' => $this->method,
            'recordName' => $this->recordName,
            'recordValue' => $this->recordValue,
            'attemptCount' => $this->attemptCount,
            'expiresAt' => $this->expiresAt,
            'nextAttemptAt' => $this->nextAttemptAt,
            'checkedAt' => $this->checkedAt,
            'failureCode' => $this->failureCode,
        ];
    }
}
