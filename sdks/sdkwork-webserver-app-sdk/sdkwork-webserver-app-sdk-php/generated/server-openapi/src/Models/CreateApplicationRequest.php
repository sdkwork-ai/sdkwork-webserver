<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

use SDKWork\Web\AppSdk\Models\ApplicationStoreListing;

final class CreateApplicationRequest
{
    public ?string $name = null;

    public ?string $slug = null;

    public ?string $description = null;

    public ?string $appKind = null;

    public array $runtimeConfig = [];

    public ?ApplicationStoreListing $storeListing = null;

    public function __construct(array $data = [])
    {
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->slug = array_key_exists('slug', $data)
            ? $data['slug']
            : null;
        $this->description = array_key_exists('description', $data)
            ? $data['description']
            : null;
        $this->appKind = array_key_exists('appKind', $data)
            ? $data['appKind']
            : null;
        $this->runtimeConfig = array_key_exists('runtimeConfig', $data)
            ? is_array($data['runtimeConfig']) ? $data['runtimeConfig'] : []
            : [];
        $this->storeListing = array_key_exists('storeListing', $data)
            ? is_array($data['storeListing']) ? ApplicationStoreListing::fromArray($data['storeListing']) : null
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'name' => $this->name,
            'slug' => $this->slug,
            'description' => $this->description,
            'appKind' => $this->appKind,
            'runtimeConfig' => $this->runtimeConfig,
            'storeListing' => $this->storeListing instanceof ApplicationStoreListing ? $this->storeListing->toArray() : $this->storeListing,
        ];
    }
}
