<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

use SDKWork\Web\AppSdk\Models\MediaResource;

final class ApplicationStoreListing
{
    public ?MediaResource $icon = null;

    public ?MediaResource $cover = null;

    public array $previews = [];

    public ?string $shortDescription = null;

    public ?string $fullDescription = null;

    public ?string $releaseNotes = null;

    public ?string $category = null;

    public array $keywords = [];

    public ?string $supportUrl = null;

    public ?string $privacyPolicyUrl = null;

    public ?string $officialWebsiteUrl = null;

    public function __construct(array $data = [])
    {
        $this->icon = array_key_exists('icon', $data)
            ? is_array($data['icon']) ? MediaResource::fromArray($data['icon']) : null
            : null;
        $this->cover = array_key_exists('cover', $data)
            ? is_array($data['cover']) ? MediaResource::fromArray($data['cover']) : null
            : null;
        $this->previews = array_key_exists('previews', $data)
            ? is_array($data['previews'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MediaResource::fromArray($item) : $item, $data['previews']))
                : []
            : [];
        $this->shortDescription = array_key_exists('shortDescription', $data)
            ? $data['shortDescription']
            : null;
        $this->fullDescription = array_key_exists('fullDescription', $data)
            ? $data['fullDescription']
            : null;
        $this->releaseNotes = array_key_exists('releaseNotes', $data)
            ? $data['releaseNotes']
            : null;
        $this->category = array_key_exists('category', $data)
            ? $data['category']
            : null;
        $this->keywords = array_key_exists('keywords', $data)
            ? is_array($data['keywords'])
                ? array_values(array_map(static fn($item) => $item, $data['keywords']))
                : []
            : [];
        $this->supportUrl = array_key_exists('supportUrl', $data)
            ? $data['supportUrl']
            : null;
        $this->privacyPolicyUrl = array_key_exists('privacyPolicyUrl', $data)
            ? $data['privacyPolicyUrl']
            : null;
        $this->officialWebsiteUrl = array_key_exists('officialWebsiteUrl', $data)
            ? $data['officialWebsiteUrl']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'icon' => $this->icon instanceof MediaResource ? $this->icon->toArray() : $this->icon,
            'cover' => $this->cover instanceof MediaResource ? $this->cover->toArray() : $this->cover,
            'previews' => array_values(array_map(static fn($item) => $item instanceof MediaResource ? $item->toArray() : $item, $this->previews)),
            'shortDescription' => $this->shortDescription,
            'fullDescription' => $this->fullDescription,
            'releaseNotes' => $this->releaseNotes,
            'category' => $this->category,
            'keywords' => array_values(array_map(static fn($item) => $item, $this->keywords)),
            'supportUrl' => $this->supportUrl,
            'privacyPolicyUrl' => $this->privacyPolicyUrl,
            'officialWebsiteUrl' => $this->officialWebsiteUrl,
        ];
    }
}
