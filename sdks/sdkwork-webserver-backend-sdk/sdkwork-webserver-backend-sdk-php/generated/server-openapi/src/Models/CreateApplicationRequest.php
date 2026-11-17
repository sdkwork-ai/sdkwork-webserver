<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\ApplicationStoreListing;

/**
 * Create input for a carrier row.

There is no `appKind` member: a carrier row's identity is
`deployApplicationId`, and its application kind is a server-owned default
(`STATIC_WEB`, which derives `siteType = 1` and the site carrier
application type `WEB`). The member was removed rather than made optional
so no caller can re-declare a second application identity on the carrier
row (`MIG-2026-0071` deletion-list item 6).
 */
final class CreateApplicationRequest
{
    public ?string $name = null;

    public ?string $slug = null;

    public ?string $description = null;

    /** The `deploy_app.id` of the sdkwork-deployments application this
carrier record carries.

Required. Deploy is the single application identity and publishing
authority; a webserver application exists only to carry a site,
domain, certificate, and reverse-proxy configuration for a Deploy
application, and the link is 1:1 (unique per live carrier row). The
service rejects a missing value with a validation error that names
this field - the webserver never mints an application identity of
its own. */
    public ?string $deployApplicationId = null;

    public array $runtimeConfig = [];

    public ?ApplicationStoreListing $storeListing = null;

    public function __construct(array $data = [])
    {
        $this->name = array_key_exists('name', $data)
            ? $data['name']
            : null;
        $this->slug = array_key_exists('slug', $data)
            ? $data['slug']
            : null;
        $this->description = array_key_exists('description', $data)
            ? $data['description']
            : null;
        $this->deployApplicationId = array_key_exists('deployApplicationId', $data)
            ? $data['deployApplicationId']
            : null;
        $this->runtimeConfig = array_key_exists('runtimeConfig', $data)
            ? is_array($data['runtimeConfig']) ? $data['runtimeConfig'] : []
            : [];
        $this->storeListing = array_key_exists('storeListing', $data)
            ? is_array($data['storeListing']) ? ApplicationStoreListing::fromArray($data['storeListing']) : null
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'name' => $this->name,
            'slug' => $this->slug,
            'description' => $this->description,
            'deployApplicationId' => $this->deployApplicationId,
            'runtimeConfig' => $this->runtimeConfig,
            'storeListing' => $this->storeListing instanceof ApplicationStoreListing ? $this->storeListing->toArray() : $this->storeListing,
        ];
    }
}
