<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class PluginResponse
{
    public ?string $id = null;

    public ?string $pluginKey = null;

    public ?string $displayName = null;

    public ?string $summary = null;

    public ?string $version = null;

    public ?string $categoryId = null;

    public array $supportedHostTools = [];

    public array $contributedCapabilities = [];

    public ?string $sourceKind = null;

    public ?string $gitRepository = null;

    public ?string $gitRef = null;

    public ?string $artifactRef = null;

    public ?string $checksumSha256 = null;

    public ?string $archiveFileName = null;

    public ?string $status = null;

    public ?string $createdAt = null;

    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->pluginKey = array_key_exists('pluginKey', $data)
            ? $data['pluginKey']
            : null;
        $this->displayName = array_key_exists('displayName', $data)
            ? $data['displayName']
            : null;
        $this->summary = array_key_exists('summary', $data)
            ? $data['summary']
            : null;
        $this->version = array_key_exists('version', $data)
            ? $data['version']
            : null;
        $this->categoryId = array_key_exists('categoryId', $data)
            ? $data['categoryId']
            : null;
        $this->supportedHostTools = array_key_exists('supportedHostTools', $data)
            ? is_array($data['supportedHostTools'])
                ? array_values(array_map(static fn($item) => $item, $data['supportedHostTools']))
                : []
            : [];
        $this->contributedCapabilities = array_key_exists('contributedCapabilities', $data)
            ? is_array($data['contributedCapabilities'])
                ? array_values(array_map(static fn($item) => $item, $data['contributedCapabilities']))
                : []
            : [];
        $this->sourceKind = array_key_exists('sourceKind', $data)
            ? $data['sourceKind']
            : null;
        $this->gitRepository = array_key_exists('gitRepository', $data)
            ? $data['gitRepository']
            : null;
        $this->gitRef = array_key_exists('gitRef', $data)
            ? $data['gitRef']
            : null;
        $this->artifactRef = array_key_exists('artifactRef', $data)
            ? $data['artifactRef']
            : null;
        $this->checksumSha256 = array_key_exists('checksumSha256', $data)
            ? $data['checksumSha256']
            : null;
        $this->archiveFileName = array_key_exists('archiveFileName', $data)
            ? $data['archiveFileName']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
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
            'pluginKey' => $this->pluginKey,
            'displayName' => $this->displayName,
            'summary' => $this->summary,
            'version' => $this->version,
            'categoryId' => $this->categoryId,
            'supportedHostTools' => array_values(array_map(static fn($item) => $item, $this->supportedHostTools)),
            'contributedCapabilities' => array_values(array_map(static fn($item) => $item, $this->contributedCapabilities)),
            'sourceKind' => $this->sourceKind,
            'gitRepository' => $this->gitRepository,
            'gitRef' => $this->gitRef,
            'artifactRef' => $this->artifactRef,
            'checksumSha256' => $this->checksumSha256,
            'archiveFileName' => $this->archiveFileName,
            'status' => $this->status,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
