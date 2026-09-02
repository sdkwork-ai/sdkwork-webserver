<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\ApplicationSourceVersionConfigSnapshot;

final class ApplicationSourceVersionResponse
{
    public ?string $id = null;

    public ?string $siteId = null;

    public ?string $versionTag = null;

    public ?string $sourceType = null;

    public ?string $sourceRef = null;

    public ?string $commitHash = null;

    public ?string $artifactDriveUri = null;

    public ?string $artifactSize = null;

    public ?string $artifactHash = null;

    public ?ApplicationSourceVersionConfigSnapshot $configSnapshot = null;

    public ?int $status = null;

    public ?bool $retained = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->siteId = array_key_exists('siteId', $data)
            ? $data['siteId']
            : null;
        $this->versionTag = array_key_exists('versionTag', $data)
            ? $data['versionTag']
            : null;
        $this->sourceType = array_key_exists('sourceType', $data)
            ? $data['sourceType']
            : null;
        $this->sourceRef = array_key_exists('sourceRef', $data)
            ? $data['sourceRef']
            : null;
        $this->commitHash = array_key_exists('commitHash', $data)
            ? $data['commitHash']
            : null;
        $this->artifactDriveUri = array_key_exists('artifactDriveUri', $data)
            ? $data['artifactDriveUri']
            : null;
        $this->artifactSize = array_key_exists('artifactSize', $data)
            ? $data['artifactSize']
            : null;
        $this->artifactHash = array_key_exists('artifactHash', $data)
            ? $data['artifactHash']
            : null;
        $this->configSnapshot = array_key_exists('configSnapshot', $data)
            ? is_array($data['configSnapshot']) ? ApplicationSourceVersionConfigSnapshot::fromArray($data['configSnapshot']) : null
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->retained = array_key_exists('retained', $data)
            ? $data['retained']
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
            'siteId' => $this->siteId,
            'versionTag' => $this->versionTag,
            'sourceType' => $this->sourceType,
            'sourceRef' => $this->sourceRef,
            'commitHash' => $this->commitHash,
            'artifactDriveUri' => $this->artifactDriveUri,
            'artifactSize' => $this->artifactSize,
            'artifactHash' => $this->artifactHash,
            'configSnapshot' => $this->configSnapshot instanceof ApplicationSourceVersionConfigSnapshot ? $this->configSnapshot->toArray() : $this->configSnapshot,
            'status' => $this->status,
            'retained' => $this->retained,
            'createdAt' => $this->createdAt,
        ];
    }
}
