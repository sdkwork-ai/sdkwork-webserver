<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class SdkWorkResourceData
{
    /** Typed domain resource for the operation. */
    public array $item = [];

    public function __construct(array $data = [])
    {
        $this->item = array_key_exists('item', $data)
            ? is_array($data['item']) ? $data['item'] : []
            : [];
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'item' => $this->item,
        ];
    }
}
