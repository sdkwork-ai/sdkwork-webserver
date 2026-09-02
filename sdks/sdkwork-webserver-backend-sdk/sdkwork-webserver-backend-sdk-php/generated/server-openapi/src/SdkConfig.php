<?php

declare(strict_types=1);

namespace SDKWork\Web\BackendSdk;

final class SdkConfig
{
    public function __construct(
        public string $baseUrl = 'http://localhost:3800',
        public int $timeout = 30,
        public array $headers = [],
        public array $transportOptions = [],
    ) {
    }
}
