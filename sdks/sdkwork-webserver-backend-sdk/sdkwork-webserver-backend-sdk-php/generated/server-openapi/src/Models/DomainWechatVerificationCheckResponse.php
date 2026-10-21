<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class DomainWechatVerificationCheckResponse
{
    /** Whether any protocol fetched a 2xx response. */
    public ?bool $reachable = null;

    /** Whether the fetched content matches the stored file byte for byte. */
    public ?bool $matched = null;

    /** HTTP status code of the matched attempt. */
    public ?int $statusCode = null;

    /** Matched scheme (https or http). */
    public ?string $scheme = null;

    /** One-line fact when the check did not pass. */
    public ?string $detail = null;

    /** Time of the check. */
    public ?string $checkedAt = null;

    public function __construct(array $data = [])
    {
        $this->reachable = array_key_exists('reachable', $data)
            ? $data['reachable']
            : null;
        $this->matched = array_key_exists('matched', $data)
            ? $data['matched']
            : null;
        $this->statusCode = array_key_exists('statusCode', $data)
            ? $data['statusCode']
            : null;
        $this->scheme = array_key_exists('scheme', $data)
            ? $data['scheme']
            : null;
        $this->detail = array_key_exists('detail', $data)
            ? $data['detail']
            : null;
        $this->checkedAt = array_key_exists('checkedAt', $data)
            ? $data['checkedAt']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'reachable' => $this->reachable,
            'matched' => $this->matched,
            'statusCode' => $this->statusCode,
            'scheme' => $this->scheme,
            'detail' => $this->detail,
            'checkedAt' => $this->checkedAt,
        ];
    }
}
