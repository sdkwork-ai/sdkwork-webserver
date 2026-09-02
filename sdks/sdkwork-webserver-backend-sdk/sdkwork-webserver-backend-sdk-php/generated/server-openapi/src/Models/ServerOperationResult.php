<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ServerOperationResult
{
    public ?string $operationId = null;

    public ?int $exitCode = null;

    public ?string $stdout = null;

    public ?string $stderr = null;

    public function __construct(array $data = [])
    {
        $this->operationId = array_key_exists('operationId', $data)
            ? $data['operationId']
            : null;
        $this->exitCode = array_key_exists('exitCode', $data)
            ? $data['exitCode']
            : null;
        $this->stdout = array_key_exists('stdout', $data)
            ? $data['stdout']
            : null;
        $this->stderr = array_key_exists('stderr', $data)
            ? $data['stderr']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'operationId' => $this->operationId,
            'exitCode' => $this->exitCode,
            'stdout' => $this->stdout,
            'stderr' => $this->stderr,
        ];
    }
}
