<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class WebserverConfigWriteRequest
{
    /** New text content; NUL-free, within the write size limit, and syntax-validated for JSON/TOML files. */
    public ?string $content = null;

    /** When provided, the write is rejected unless the on-disk digest still matches (optimistic concurrency). */
    public ?string $expectedSha256 = null;

    public function __construct(array $data = [])
    {
        $this->content = array_key_exists('content', $data)
            ? $data['content']
            : null;
        $this->expectedSha256 = array_key_exists('expectedSha256', $data)
            ? $data['expectedSha256']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'content' => $this->content,
            'expectedSha256' => $this->expectedSha256,
        ];
    }
}
