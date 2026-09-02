<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class UpdateEnvVariableRequest
{
    public ?string $value = null;

    public ?bool $isSecret = null;

    public function __construct(array $data = [])
    {
        $this->value = array_key_exists('value', $data)
            ? $data['value']
            : null;
        $this->isSecret = array_key_exists('isSecret', $data)
            ? $data['isSecret']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'value' => $this->value,
            'isSecret' => $this->isSecret,
        ];
    }
}
