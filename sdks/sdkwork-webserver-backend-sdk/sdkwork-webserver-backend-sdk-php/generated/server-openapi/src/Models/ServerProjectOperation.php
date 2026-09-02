<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ServerProjectOperation
{
    public ?string $id = null;

    public ?string $kind = null;

    public ?string $label = null;

    /** IAM permission required to invoke the operation. */
    public ?string $permission = null;

    public ?string $description = null;

    public ?bool $dangerous = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->kind = array_key_exists('kind', $data)
            ? $data['kind']
            : null;
        $this->label = array_key_exists('label', $data)
            ? $data['label']
            : null;
        $this->permission = array_key_exists('permission', $data)
            ? $data['permission']
            : null;
        $this->description = array_key_exists('description', $data)
            ? $data['description']
            : null;
        $this->dangerous = array_key_exists('dangerous', $data)
            ? $data['dangerous']
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
            'kind' => $this->kind,
            'label' => $this->label,
            'permission' => $this->permission,
            'description' => $this->description,
            'dangerous' => $this->dangerous,
        ];
    }
}
