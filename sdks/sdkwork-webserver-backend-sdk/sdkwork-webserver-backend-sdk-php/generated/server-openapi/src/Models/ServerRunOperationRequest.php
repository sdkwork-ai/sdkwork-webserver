<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class ServerRunOperationRequest
{
    public ?string $path = null;

    public ?string $operationId = null;

    public function __construct(array $data = [])
    {
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->operationId = array_key_exists('operationId', $data)
            ? $data['operationId']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'path' => $this->path,
            'operationId' => $this->operationId,
        ];
    }
}
