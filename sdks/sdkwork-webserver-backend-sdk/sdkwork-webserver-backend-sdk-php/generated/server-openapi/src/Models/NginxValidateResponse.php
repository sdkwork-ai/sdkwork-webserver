<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class NginxValidateResponse
{
    public ?bool $valid = null;

    public array $errors = [];

    public function __construct(array $data = [])
    {
        $this->valid = array_key_exists('valid', $data)
            ? $data['valid']
            : null;
        $this->errors = array_key_exists('errors', $data)
            ? is_array($data['errors'])
                ? array_values(array_map(static fn($item) => is_array($item) ? $item : [], $data['errors']))
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
            'valid' => $this->valid,
            'errors' => array_values(array_map(static fn($item) => $item, $this->errors)),
        ];
    }
}
