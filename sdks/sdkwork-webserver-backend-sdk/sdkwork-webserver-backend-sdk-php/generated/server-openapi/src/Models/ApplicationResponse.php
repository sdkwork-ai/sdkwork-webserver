<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\ApplicationStoreListing;

final class ApplicationResponse
{
    public ?string $id = null;

    public ?string $name = null;

    public ?string $slug = null;

    public ?string $description = null;

    public ?string $appKind = null;

    public ?int $siteType = null;

    public ?int $status = null;

    public array $runtimeConfig = [];

    public ?ApplicationStoreListing $storeListing = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
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
        $this->siteType = array_key_exists('siteType', $data)
            ? $data['siteType']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->runtimeConfig = array_key_exists('runtimeConfig', $data)
            ? is_array($data['runtimeConfig']) ? $data['runtimeConfig'] : []
            : [];
        $this->storeListing = array_key_exists('storeListing', $data)
            ? is_array($data['storeListing']) ? ApplicationStoreListing::fromArray($data['storeListing']) : null
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
            : null;
        $this->updatedAt = array_key_exists('updatedAt', $data)
            ? $data['updatedAt']
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
            'name' => $this->name,
            'slug' => $this->slug,
            'description' => $this->description,
            'appKind' => $this->appKind,
            'siteType' => $this->siteType,
            'status' => $this->status,
            'runtimeConfig' => $this->runtimeConfig,
            'storeListing' => $this->storeListing instanceof ApplicationStoreListing ? $this->storeListing->toArray() : $this->storeListing,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
