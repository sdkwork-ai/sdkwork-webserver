<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class ApplicationDeploymentResponse
{
    public ?string $id = null;

    public ?string $siteId = null;

    public ?string $sourceVersionId = null;

    public ?int $status = null;

    public ?int $deployType = null;

    public ?string $environment = null;

    public ?string $versionTag = null;

    public ?string $commitHash = null;

    public ?string $sourceRef = null;

    /** Immutable successful deployment selected as this restore command's source. */
    public ?string $rollbackFromDeploymentId = null;

    public ?string $artifactDriveUri = null;

    public ?string $artifactSize = null;

    public ?string $artifactHash = null;

    public ?string $startedAt = null;

    public ?string $completedAt = null;

    public ?string $durationMs = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->siteId = array_key_exists('siteId', $data)
            ? $data['siteId']
            : null;
        $this->sourceVersionId = array_key_exists('sourceVersionId', $data)
            ? $data['sourceVersionId']
            : null;
        $this->status = array_key_exists('status', $data)
            ? $data['status']
            : null;
        $this->deployType = array_key_exists('deployType', $data)
            ? $data['deployType']
            : null;
        $this->environment = array_key_exists('environment', $data)
            ? $data['environment']
            : null;
        $this->versionTag = array_key_exists('versionTag', $data)
            ? $data['versionTag']
            : null;
        $this->commitHash = array_key_exists('commitHash', $data)
            ? $data['commitHash']
            : null;
        $this->sourceRef = array_key_exists('sourceRef', $data)
            ? $data['sourceRef']
            : null;
        $this->rollbackFromDeploymentId = array_key_exists('rollbackFromDeploymentId', $data)
            ? $data['rollbackFromDeploymentId']
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
        $this->startedAt = array_key_exists('startedAt', $data)
            ? $data['startedAt']
            : null;
        $this->completedAt = array_key_exists('completedAt', $data)
            ? $data['completedAt']
            : null;
        $this->durationMs = array_key_exists('durationMs', $data)
            ? $data['durationMs']
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
            'sourceVersionId' => $this->sourceVersionId,
            'status' => $this->status,
            'deployType' => $this->deployType,
            'environment' => $this->environment,
            'versionTag' => $this->versionTag,
            'commitHash' => $this->commitHash,
            'sourceRef' => $this->sourceRef,
            'rollbackFromDeploymentId' => $this->rollbackFromDeploymentId,
            'artifactDriveUri' => $this->artifactDriveUri,
            'artifactSize' => $this->artifactSize,
            'artifactHash' => $this->artifactHash,
            'startedAt' => $this->startedAt,
            'completedAt' => $this->completedAt,
            'durationMs' => $this->durationMs,
            'createdAt' => $this->createdAt,
        ];
    }
}
