<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class CreateDomainVerificationFileRequest
{
    /** Issuing platform. */
    public ?string $platform = null;

    /** Single-segment ASCII file name ending with .txt, .html or .xml. */
    public ?string $fileName = null;

    /** Verbatim file content, stored and returned byte for byte. */
    public ?string $content = null;

    public function __construct(array $data = [])
    {
        $this->platform = array_key_exists('platform', $data)
            ? $data['platform']
            : null;
        $this->fileName = array_key_exists('fileName', $data)
            ? $data['fileName']
            : null;
        $this->content = array_key_exists('content', $data)
            ? $data['content']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'platform' => $this->platform,
            'fileName' => $this->fileName,
            'content' => $this->content,
        ];
    }
}
