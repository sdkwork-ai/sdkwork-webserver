<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class FieldError
{
    public ?string $field = null;

    public ?string $message = null;

    public ?int $code = null;

    public function __construct(array $data = [])
    {
        $this->field = array_key_exists('field', $data)
            ? $data['field']
            : null;
        $this->message = array_key_exists('message', $data)
            ? $data['message']
            : null;
        $this->code = array_key_exists('code', $data)
            ? $data['code']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'field' => $this->field,
            'message' => $this->message,
            'code' => $this->code,
        ];
    }
}
