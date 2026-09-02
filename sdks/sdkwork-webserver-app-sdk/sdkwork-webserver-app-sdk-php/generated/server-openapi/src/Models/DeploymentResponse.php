<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class DeploymentResponse
{
    public ?string $id = null;

    public ?string $applicationId = null;

    public ?int $deployType = null;

    public ?string $sourceVersionId = null;

    public ?string $versionTag = null;

    public ?string $commitHash = null;

    public ?string $sourceRef = null;

    /** 此还原命令所引用的不可变历史成功版本 ID。 */
    public ?string $rollbackFromDeploymentId = null;

    public ?string $environment = null;

    public ?string $artifactDriveUri = null;

    public ?string $artifactSize = null;

    public ?string $artifactHash = null;

    public ?int $status = null;

    public ?string $startedAt = null;

    public ?string $completedAt = null;

    /** Deployment duration in milliseconds as a string to avoid JavaScript precision loss. */
    public ?string $durationMs = null;

    public ?string $createdAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->applicationId = array_key_exists('applicationId', $data)
            ? $data['applicationId']
            : null;
        $this->deployType = array_key_exists('deployType', $data)
            ? $data['deployType']
            : null;
        $this->sourceVersionId = array_key_exists('sourceVersionId', $data)
            ? $data['sourceVersionId']
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
        $this->environment = array_key_exists('environment', $data)
            ? $data['environment']
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
        $this->status = array_key_exists('status', $data)
            ? $data['status']
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
            'applicationId' => $this->applicationId,
            'deployType' => $this->deployType,
            'sourceVersionId' => $this->sourceVersionId,
            'versionTag' => $this->versionTag,
            'commitHash' => $this->commitHash,
            'sourceRef' => $this->sourceRef,
            'rollbackFromDeploymentId' => $this->rollbackFromDeploymentId,
            'environment' => $this->environment,
            'artifactDriveUri' => $this->artifactDriveUri,
            'artifactSize' => $this->artifactSize,
            'artifactHash' => $this->artifactHash,
            'status' => $this->status,
            'startedAt' => $this->startedAt,
            'completedAt' => $this->completedAt,
            'durationMs' => $this->durationMs,
            'createdAt' => $this->createdAt,
        ];
    }
}
