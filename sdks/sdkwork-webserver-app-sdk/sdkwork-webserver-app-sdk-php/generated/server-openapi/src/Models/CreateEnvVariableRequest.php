<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class CreateEnvVariableRequest
{
    public ?string $key = null;

    public ?string $value = null;

    public ?string $environment = null;

    public ?bool $isSecret = null;

    public function __construct(array $data = [])
    {
        $this->key = array_key_exists('key', $data)
            ? $data['key']
            : null;
        $this->value = array_key_exists('value', $data)
            ? $data['value']
            : null;
        $this->environment = array_key_exists('environment', $data)
            ? $data['environment']
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
            'key' => $this->key,
            'value' => $this->value,
            'environment' => $this->environment,
            'isSecret' => $this->isSecret,
        ];
    }
}
