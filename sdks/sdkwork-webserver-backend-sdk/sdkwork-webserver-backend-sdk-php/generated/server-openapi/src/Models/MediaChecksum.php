<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class MediaChecksum
{
    public ?string $algorithm = null;

    public ?string $value = null;

    public function __construct(array $data = [])
    {
        $this->algorithm = array_key_exists('algorithm', $data)
            ? $data['algorithm']
            : null;
        $this->value = array_key_exists('value', $data)
            ? $data['value']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'algorithm' => $this->algorithm,
            'value' => $this->value,
        ];
    }
}
