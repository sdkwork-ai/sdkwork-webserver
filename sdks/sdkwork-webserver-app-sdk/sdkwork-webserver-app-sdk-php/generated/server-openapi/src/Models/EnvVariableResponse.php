<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class EnvVariableResponse
{
    public ?string $id = null;

    public ?string $key = null;

    public ?string $environment = null;

    public ?bool $isSecret = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->key = array_key_exists('key', $data)
            ? $data['key']
            : null;
        $this->environment = array_key_exists('environment', $data)
            ? $data['environment']
            : null;
        $this->isSecret = array_key_exists('isSecret', $data)
            ? $data['isSecret']
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
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
            'key' => $this->key,
            'environment' => $this->environment,
            'isSecret' => $this->isSecret,
            'createdAt' => $this->createdAt,
        ];
    }
}
