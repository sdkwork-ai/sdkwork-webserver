<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class DomainVerificationFileResponse
{
    public ?string $id = null;

    public ?string $rootDomainId = null;

    /** Issuing platform. Grouping/audit only; the edge matches by file name. */
    public ?string $platform = null;

    /** File name issued by the platform (for example MP_verify_xxx.txt, google123.html); served by exact name. */
    public ?string $fileName = null;

    /** Verbatim file content, stored and returned byte for byte. */
    public ?string $content = null;

    /** Time the file was first registered. */
    public ?string $createdAt = null;

    /** Time of the most recent upload or replacement. */
    public ?string $updatedAt = null;

    public function __construct(array $data = [])
    {
        $this->id = array_key_exists('id', $data)
            ? $data['id']
            : null;
        $this->rootDomainId = array_key_exists('rootDomainId', $data)
            ? $data['rootDomainId']
            : null;
        $this->platform = array_key_exists('platform', $data)
            ? $data['platform']
            : null;
        $this->fileName = array_key_exists('fileName', $data)
            ? $data['fileName']
            : null;
        $this->content = array_key_exists('content', $data)
            ? $data['content']
            : null;
        $this->createdAt = array_key_exists('createdAt', $data)
            ? $data['createdAt']
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
            'id' => $this->id,
            'rootDomainId' => $this->rootDomainId,
            'platform' => $this->platform,
            'fileName' => $this->fileName,
            'content' => $this->content,
            'createdAt' => $this->createdAt,
            'updatedAt' => $this->updatedAt,
        ];
    }
}
