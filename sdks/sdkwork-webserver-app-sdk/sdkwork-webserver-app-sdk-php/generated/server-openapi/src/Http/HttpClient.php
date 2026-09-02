<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Http;

use SDKWork\Web\AppSdk\SdkConfig;
use GuzzleHttp\Client;
use GuzzleHttp\Exception\GuzzleException;
use RuntimeException;

final class HttpClient
{
    private Client $client;
    private array $headers;
    private ?string $apiKey = null;
    private ?string $authToken = null;
    private ?string $accessToken = null;

    public function __construct(private SdkConfig $config)
    {
        $this->headers = $config->headers;
        $this->client = new Client(array_merge([
            'base_uri' => rtrim($config->baseUrl, '/'),
            'timeout' => $config->timeout,
        ], $config->transportOptions));
    }

    public function setApiKey(string $apiKey): self
    {
        $this->apiKey = $apiKey;
        $this->authToken = null;
        $this->accessToken = null;
        return $this;
    }

    public function setAuthToken(string $token): self
    {
        $this->authToken = $token;
        if (strtolower('X-API-Key') !== 'authorization') {
            $this->apiKey = null;
        }
        return $this;
    }

    public function setAccessToken(string $token): self
    {
        $this->accessToken = $token;
        if (strtolower('X-API-Key') !== 'access-token') {
            $this->apiKey = null;
        }
        return $this;
    }

    public function setHeader(string $key, string $value): self
    {
        $this->headers[$key] = $value;
        return $this;
    }

    public function request(string $method, string $path, array $options = []): mixed
    {
        $requestOptions = [];
        $requestOptions['headers'] = $this->resolveHeaders($options);

        if (!empty($options['query'])) {
            $requestOptions['query'] = $options['query'];
        }
        if (array_key_exists('json', $options)) {
            $requestOptions['json'] = $options['json'];
        }
        if (!empty($options['form_params'])) {
            $requestOptions['form_params'] = $options['form_params'];
        }
        if (!empty($options['multipart'])) {
            $requestOptions['multipart'] = $this->normalizeMultipart($options['multipart']);
        }

        try {
            $response = $this->client->request($method, $path, $requestOptions);
        } catch (GuzzleException $exception) {
            throw new RuntimeException('SDK request failed: ' . $exception->getMessage(), (int) $exception->getCode(), $exception);
        }

        $body = (string) $response->getBody();
        if ($body === '') {
            return null;
        }

        $decoded = json_decode($body, true);
        if (json_last_error() === JSON_ERROR_NONE) {
            return $decoded;
        }

        return $body;
    }

    public function stream(string $method, string $path, array $options = []): \Generator
    {
        $requestOptions = [
            'stream' => true,
        ];
        $requestOptions['headers'] = array_merge(
            ['Accept' => 'text/event-stream'],
            $this->resolveHeaders($options)
        );

        if (!empty($options['query'])) {
            $requestOptions['query'] = $options['query'];
        }
        if (array_key_exists('json', $options)) {
            $requestOptions['json'] = $options['json'];
        }
        if (!empty($options['form_params'])) {
            $requestOptions['form_params'] = $options['form_params'];
        }
        if (!empty($options['multipart'])) {
            $requestOptions['multipart'] = $this->normalizeMultipart($options['multipart']);
        }

        try {
            $response = $this->client->request($method, $path, $requestOptions);
        } catch (GuzzleException $exception) {
            throw new RuntimeException('SDK stream failed: ' . $exception->getMessage(), (int) $exception->getCode(), $exception);
        }

        $buffer = '';
        while (!$response->getBody()->eof()) {
            $buffer .= $response->getBody()->read(8192);
            while (preg_match('/\r?\n\r?\n/', $buffer, $matches, PREG_OFFSET_CAPTURE)) {
                $position = $matches[0][1];
                $delimiterLength = strlen($matches[0][0]);
                $rawEvent = substr($buffer, 0, $position);
                $buffer = substr($buffer, $position + $delimiterLength);
                foreach ($this->parseSseEvent($rawEvent) as $event) {
                    yield $event;
                }
            }
        }

        if (trim($buffer) !== '') {
            foreach ($this->parseSseEvent($buffer) as $event) {
                yield $event;
            }
        }
    }

    private function resolveHeaders(array $options): array
    {
        $skipAuth = !empty($options['skipAuth']);
        $accessTokenOnly = !empty($options['accessTokenOnly']);
        $requestHeaders = $options['headers'] ?? [];
        if (!$skipAuth && !$accessTokenOnly) {
            return array_merge($this->buildAuthHeaders(), $this->headers, $requestHeaders);
        }

        $resolved = $this->stripCredentialHeaders($requestHeaders);
        if ($accessTokenOnly) {
            $accessToken = trim($this->accessToken ?? '');
            if ($accessToken === '') {
                throw new RuntimeException(
                    'access-token-only request requires Access-Token before request dispatch'
                );
            }
            $resolved['Access-Token'] = $accessToken;
        }
        return $resolved;
    }

    private function stripCredentialHeaders(array $headers): array
    {
        $forbidden = [
            'authorization', 'access-token', 'x-api-key', 'x-tenant-id',
            'x-organization-id', 'x-platform', 'x-user-id',
            'x-sdkwork-tenant-id', 'x-sdkwork-organization-id', 'x-sdkwork-user-id',
        ];
        return array_filter(
            $headers,
            static fn(string $key): bool => !in_array(strtolower($key), $forbidden, true),
            ARRAY_FILTER_USE_KEY
        );
    }

    private function buildAuthHeaders(): array
    {
        $headers = [];

        if ($this->apiKey !== null && $this->apiKey !== '') {
            $headers['X-API-Key'] = $this->apiKey;
        }
        if ($this->authToken !== null && $this->authToken !== '') {
            $headers['Authorization'] = $this->formatBearer($this->authToken);
        }
        if ($this->accessToken !== null && $this->accessToken !== '') {
            $headers['Access-Token'] = $this->accessToken;
        }

        return $headers;
    }

    private function normalizeMultipart(mixed $payload): array
    {
        if (!is_array($payload)) {
            return [];
        }

        $parts = [];
        foreach ($payload as $name => $value) {
            if (is_array($value)) {
                $parts[] = [
                    'name' => (string) $name,
                    'contents' => json_encode($value, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES),
                ];
                continue;
            }

            $parts[] = [
                'name' => (string) $name,
                'contents' => is_string($value) || is_resource($value) ? $value : (string) $value,
            ];
        }

        return $parts;
    }

    private function parseSseEvent(string $rawEvent): array
    {
        $dataLines = [];
        foreach (preg_split('/\r?\n/', $rawEvent) ?: [] as $line) {
            if (str_starts_with($line, 'data:')) {
                $dataLines[] = ltrim(substr($line, 5));
            }
        }

        if ($dataLines === []) {
            return [];
        }

        $data = implode("\n", $dataLines);
        if ($data === '[DONE]') {
            return [];
        }

        $decoded = json_decode($data, true);
        if (json_last_error() !== JSON_ERROR_NONE) {
            return [];
        }

        return [$decoded];
    }

    private function formatBearer(string $value): string
    {
        return 'Bearer ' . $value;
    }
}
