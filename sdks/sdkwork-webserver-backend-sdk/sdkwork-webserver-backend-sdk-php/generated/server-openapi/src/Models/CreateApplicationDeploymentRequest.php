<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

/**
 * Deployment source command. Git deployments (deployType 2) require an HTTPS sourceRef and may omit artifact fields. Other deployment types require artifactDriveUri, artifactSize, and artifactHash together.
 */
final class CreateApplicationDeploymentRequest
{
    /** Ready, retained application source version selected for this release. */
    public ?string $sourceVersionId = null;

    /** 1 for a stored package, 2 for a Git repository, 3 for CI/CD, or 4 for API delivery. */
    public ?int $deployType = null;

    public ?string $environment = null;

    public ?string $versionTag = null;

    public ?string $commitHash = null;

    /** HTTPS Git repository URL when deployType is 2. Credentials, query parameters, and fragments are forbidden. */
    public ?string $sourceRef = null;

    /** Stable Drive resource identity for package deployments. Signed delivery URLs are forbidden. */
    public ?string $artifactDriveUri = null;

    public ?string $artifactSize = null;

    /** Lowercase SHA-256 hexadecimal digest of the uploaded package. */
    public ?string $artifactHash = null;

    public function __construct(array $data = [])
    {
        $this->sourceVersionId = array_key_exists('sourceVersionId', $data)
            ? $data['sourceVersionId']
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
        $this->artifactDriveUri = array_key_exists('artifactDriveUri', $data)
            ? $data['artifactDriveUri']
            : null;
        $this->artifactSize = array_key_exists('artifactSize', $data)
            ? $data['artifactSize']
            : null;
        $this->artifactHash = array_key_exists('artifactHash', $data)
            ? $data['artifactHash']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'sourceVersionId' => $this->sourceVersionId,
            'deployType' => $this->deployType,
            'environment' => $this->environment,
            'versionTag' => $this->versionTag,
            'commitHash' => $this->commitHash,
            'sourceRef' => $this->sourceRef,
            'artifactDriveUri' => $this->artifactDriveUri,
            'artifactSize' => $this->artifactSize,
            'artifactHash' => $this->artifactHash,
        ];
    }
}
