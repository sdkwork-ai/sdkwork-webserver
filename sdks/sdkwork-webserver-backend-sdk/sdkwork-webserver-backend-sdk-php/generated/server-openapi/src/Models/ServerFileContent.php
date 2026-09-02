<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk\Models;

final class ServerFileContent
{
    public ?string $nodeId = null;

    public ?string $path = null;

    /** Decoded text content, bounded by the node read size limit. */
    public ?string $content = null;

    public ?string $size = null;

    public function __construct(array $data = [])
    {
        $this->nodeId = array_key_exists('nodeId', $data)
            ? $data['nodeId']
            : null;
        $this->path = array_key_exists('path', $data)
            ? $data['path']
            : null;
        $this->content = array_key_exists('content', $data)
            ? $data['content']
            : null;
        $this->size = array_key_exists('size', $data)
            ? $data['size']
            : null;
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
            'content' => $this->content,
            'size' => $this->size,
        ];
    }
}
