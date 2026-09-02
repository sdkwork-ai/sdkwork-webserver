<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\FieldError;

final class ProblemDetail
{
    public ?string $type = null;

    public ?string $title = null;

    public ?int $status = null;

    public ?string $detail = null;

    public ?string $instance = null;

    /** Platform or domain error code per API_SPEC.md section 15.3. */
    public ?int $code = null;

    /** Server-owned request correlation id. */
    public ?string $traceId = null;

    public array $errors = [];

    public function __construct(array $data = [])
    {
        $this->type = array_key_exists('type', $data)
            ? $data['type']
            : null;
        $this->title = array_key_exists('title', $data)
            ? $data['title']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->detail = array_key_exists('detail', $data)
            ? $data['detail']
            : null;
        $this->instance = array_key_exists('instance', $data)
            ? $data['instance']
            : null;
        $this->code = array_key_exists('code', $data)
            ? $data['code']
            : null;
        $this->traceId = array_key_exists('traceId', $data)
            ? $data['traceId']
            : null;
        $this->errors = array_key_exists('errors', $data)
            ? is_array($data['errors'])
                ? array_values(array_map(static fn($item) => is_array($item) ? FieldError::fromArray($item) : $item, $data['errors']))
                : []
            : [];
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'type' => $this->type,
            'title' => $this->title,
            'status' => $this->status,
            'detail' => $this->detail,
            'instance' => $this->instance,
            'code' => $this->code,
            'traceId' => $this->traceId,
            'errors' => array_values(array_map(static fn($item) => $item instanceof FieldError ? $item->toArray() : $item, $this->errors)),
        ];
    }
}
