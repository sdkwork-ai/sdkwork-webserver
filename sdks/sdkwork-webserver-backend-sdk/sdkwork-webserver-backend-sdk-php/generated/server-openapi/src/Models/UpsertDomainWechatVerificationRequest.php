<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class UpsertDomainWechatVerificationRequest
{
    /** File name issued by the platform; must end with .txt. */
    public ?string $fileName = null;

    /** Verbatim file content, stored and returned byte for byte. */
    public ?string $content = null;

    public function __construct(array $data = [])
    {
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
            'fileName' => $this->fileName,
            'content' => $this->content,
        ];
    }
}
