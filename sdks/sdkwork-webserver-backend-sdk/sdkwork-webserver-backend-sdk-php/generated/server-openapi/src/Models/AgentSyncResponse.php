<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

use SDKWork\Web\BackendSdk\Models\AgentCertificateBundle;
use SDKWork\Web\BackendSdk\Models\AgentNginxConfigBundle;

final class AgentSyncResponse
{
    public ?string $serverId = null;

    /** Stable SHA-256 fingerprint of active nginx configs and certificates for the tenant. */
    public ?string $syncVersion = null;

    /** True when ifSyncVersion matched syncVersion; bundles are omitted to save bandwidth. */
    public ?bool $unchanged = null;

    public array $nginxConfigs = [];

    public array $certificates = [];

    public function __construct(array $data = [])
    {
        $this->serverId = array_key_exists('serverId', $data)
            ? $data['serverId']
            : null;
        $this->syncVersion = array_key_exists('syncVersion', $data)
            ? $data['syncVersion']
            : null;
        $this->unchanged = array_key_exists('unchanged', $data)
            ? $data['unchanged']
            : null;
        $this->nginxConfigs = array_key_exists('nginxConfigs', $data)
            ? is_array($data['nginxConfigs'])
                ? array_values(array_map(static fn($item) => is_array($item) ? AgentNginxConfigBundle::fromArray($item) : $item, $data['nginxConfigs']))
                : []
            : [];
        $this->certificates = array_key_exists('certificates', $data)
            ? is_array($data['certificates'])
                ? array_values(array_map(static fn($item) => is_array($item) ? AgentCertificateBundle::fromArray($item) : $item, $data['certificates']))
                : []
            : [];
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'serverId' => $this->serverId,
            'syncVersion' => $this->syncVersion,
            'unchanged' => $this->unchanged,
            'nginxConfigs' => array_values(array_map(static fn($item) => $item instanceof AgentNginxConfigBundle ? $item->toArray() : $item, $this->nginxConfigs)),
            'certificates' => array_values(array_map(static fn($item) => $item instanceof AgentCertificateBundle ? $item->toArray() : $item, $this->certificates)),
        ];
    }
}
