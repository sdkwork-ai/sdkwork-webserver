<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class DomainWechatVerificationResponse
{
    public ?string $rootDomainId = null;

    /** File name issued by the platform (for example MP_verify_xxx.txt); served by exact name. */
    public ?string $fileName = null;

    /** Verbatim file content, stored and returned byte for byte. */
    public ?string $content = null;

    /** Time of the most recent upload or replacement. */
    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->rootDomainId = array_key_exists('rootDomainId', $data)
            ? $data['rootDomainId']
            : null;
        $this->fileName = array_key_exists('fileName', $data)
            ? $data['fileName']
            : null;
        $this->content = array_key_exists('content', $data)
            ? $data['content']
            : null;
        $this->updatedAt = array_key_exists('updatedAt', $data)
            ? $data['updatedAt']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'rootDomainId' => $this->rootDomainId,
            'fileName' => $this->fileName,
            'content' => $this->content,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
