<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\AgentSyncResponse;

final class RetrieveResponse
{
    public ?int $code = null;

    public mixed $data = null;

    /** Server-owned request correlation id. */
    public ?string $traceId = null;

    public function __construct(array $data = [])
    {
        $this->code = array_key_exists('code', $data)
            ? $data['code']
            : null;
        $this->data = array_key_exists('data', $data)
            ? $data['data']
            : null;
        $this->traceId = array_key_exists('traceId', $data)
            ? $data['traceId']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'code' => $this->code,
            'data' => $this->data,
            'traceId' => $this->traceId,
        ];
    }
}
