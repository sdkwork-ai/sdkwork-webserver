<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\PageInfo;
use SDKWork\Webserver\BackendSdk\Models\TunnelMachineResponse;

final class TunnelMachineListResponse
{
    public array $machines = [];

    public ?PageInfo $pageInfo = null;

    public function __construct(array $data = [])
    {
        $this->machines = array_key_exists('machines', $data)
            ? is_array($data['machines'])
                ? array_values(array_map(static fn($item) => is_array($item) ? TunnelMachineResponse::fromArray($item) : $item, $data['machines']))
                : []
            : [];
        $this->pageInfo = array_key_exists('pageInfo', $data)
            ? is_array($data['pageInfo']) ? PageInfo::fromArray($data['pageInfo']) : null
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'machines' => array_values(array_map(static fn($item) => $item instanceof TunnelMachineResponse ? $item->toArray() : $item, $this->machines)),
            'pageInfo' => $this->pageInfo instanceof PageInfo ? $this->pageInfo->toArray() : $this->pageInfo,
        ];
    }
}
