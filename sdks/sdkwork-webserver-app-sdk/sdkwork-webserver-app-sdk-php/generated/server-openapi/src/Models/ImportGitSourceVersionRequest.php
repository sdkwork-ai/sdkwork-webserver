<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Models;

final class ImportGitSourceVersionRequest
{
    public ?string $versionTag = null;

    public ?string $repositoryUrl = null;

    public ?string $gitRef = null;

    public function __construct(array $data = [])
    {
        $this->versionTag = array_key_exists('versionTag', $data)
            ? $data['versionTag']
            : null;
        $this->repositoryUrl = array_key_exists('repositoryUrl', $data)
            ? $data['repositoryUrl']
            : null;
        $this->gitRef = array_key_exists('gitRef', $data)
            ? $data['gitRef']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'versionTag' => $this->versionTag,
            'repositoryUrl' => $this->repositoryUrl,
            'gitRef' => $this->gitRef,
        ];
    }
}
