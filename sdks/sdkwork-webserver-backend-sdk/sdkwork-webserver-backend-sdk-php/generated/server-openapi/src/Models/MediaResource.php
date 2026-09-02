<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\MediaChecksum;

final class MediaResource
{
    public ?string $id = null;

    public ?string $kind = null;

    public ?string $source = null;

    public ?string $url = null;

    public ?string $publicUrl = null;

    public ?string $uri = null;

    public ?string $objectBlobId = null;

    public ?string $fileName = null;

    public ?string $mimeType = null;

    public ?string $sizeBytes = null;

    public ?MediaChecksum $checksum = null;

    public ?int $width = null;

    public ?int $height = null;

    public ?float $durationSeconds = null;

    public ?string $altText = null;

    public ?string $title = null;

    public array $metadata = [];

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->kind = array_key_exists('kind', $data)
            ? $data['kind']
            : null;
        $this->source = array_key_exists('source', $data)
            ? $data['source']
            : null;
        $this->url = array_key_exists('url', $data)
            ? $data['url']
            : null;
        $this->publicUrl = array_key_exists('publicUrl', $data)
            ? $data['publicUrl']
            : null;
        $this->uri = array_key_exists('uri', $data)
            ? $data['uri']
            : null;
        $this->objectBlobId = array_key_exists('objectBlobId', $data)
            ? $data['objectBlobId']
            : null;
        $this->fileName = array_key_exists('fileName', $data)
            ? $data['fileName']
            : null;
        $this->mimeType = array_key_exists('mimeType', $data)
            ? $data['mimeType']
            : null;
        $this->sizeBytes = array_key_exists('sizeBytes', $data)
            ? $data['sizeBytes']
            : null;
        $this->checksum = array_key_exists('checksum', $data)
            ? is_array($data['checksum']) ? MediaChecksum::fromArray($data['checksum']) : null
            : null;
        $this->width = array_key_exists('width', $data)
            ? $data['width']
            : null;
        $this->height = array_key_exists('height', $data)
            ? $data['height']
            : null;
        $this->durationSeconds = array_key_exists('durationSeconds', $data)
            ? $data['durationSeconds']
            : null;
        $this->altText = array_key_exists('altText', $data)
            ? $data['altText']
            : null;
        $this->title = array_key_exists('title', $data)
            ? $data['title']
            : null;
        $this->metadata = array_key_exists('metadata', $data)
            ? is_array($data['metadata']) ? $data['metadata'] : []
            : [];
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
            'source' => $this->source,
            'url' => $this->url,
            'publicUrl' => $this->publicUrl,
            'uri' => $this->uri,
            'objectBlobId' => $this->objectBlobId,
            'fileName' => $this->fileName,
            'mimeType' => $this->mimeType,
            'sizeBytes' => $this->sizeBytes,
            'checksum' => $this->checksum instanceof MediaChecksum ? $this->checksum->toArray() : $this->checksum,
            'width' => $this->width,
            'height' => $this->height,
            'durationSeconds' => $this->durationSeconds,
            'altText' => $this->altText,
            'title' => $this->title,
            'metadata' => $this->metadata,
        ];
    }
}
