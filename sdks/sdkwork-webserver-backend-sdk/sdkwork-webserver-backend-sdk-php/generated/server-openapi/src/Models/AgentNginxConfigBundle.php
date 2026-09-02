<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class AgentNginxConfigBundle
{
    public ?string $configId = null;

    public ?string $domain = null;

    public ?string $configContent = null;

    /** SHA-256 hex digest of configContent. */
    public ?string $fingerprint = null;

    /** Config revision number as a string to avoid JavaScript precision loss. */
    public ?string $version = null;

    public function __construct(array $data = [])
    {
        $this->configId = array_key_exists('configId', $data)
            ? $data['configId']
            : null;
        $this->domain = array_key_exists('domain', $data)
            ? $data['domain']
            : null;
        $this->configContent = array_key_exists('configContent', $data)
            ? $data['configContent']
            : null;
        $this->fingerprint = array_key_exists('fingerprint', $data)
            ? $data['fingerprint']
            : null;
        $this->version = array_key_exists('version', $data)
            ? $data['version']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'configId' => $this->configId,
            'domain' => $this->domain,
            'configContent' => $this->configContent,
            'fingerprint' => $this->fingerprint,
            'version' => $this->version,
        ];
    }
}
