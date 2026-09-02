<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\ServerProjectOperation;

final class ServerProjectOperations
{
    public ?string $nodeId = null;

    public ?string $path = null;

    public ?string $projectType = null;

    public array $operations = [];

    public function __construct(array $data = [])
    {
        $this->nodeId = array_key_exists('nodeId', $data)
            ? $data['nodeId']
            : null;
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->projectType = array_key_exists('projectType', $data)
            ? $data['projectType']
            : null;
        $this->operations = array_key_exists('operations', $data)
            ? is_array($data['operations'])
                ? array_values(array_map(static fn($item) => is_array($item) ? ServerProjectOperation::fromArray($item) : $item, $data['operations']))
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
            'nodeId' => $this->nodeId,
            'path' => $this->path,
            'projectType' => $this->projectType,
            'operations' => array_values(array_map(static fn($item) => $item instanceof ServerProjectOperation ? $item->toArray() : $item, $this->operations)),
        ];
    }
}
