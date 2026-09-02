<?php

declare(strict_types=1);

namespace SDKWork\Web\AppSdk\Api;

use SDKWork\Web\AppSdk\Http\HttpClient;

abstract class BaseApi
{
    public function __construct(protected HttpClient $client)
    {
    }

    protected function interpolatePath(string $path, array $pathParams): string
    {
        foreach ($pathParams as $name => $value) {
            $path = str_replace('{' . $name . '}', rawurlencode((string) $value), $path);
        }

        return $path;
    }

    protected function serializePathParameter(mixed $value, PathParameterSpec $spec): string
    {
        if ($value === null) {
            return '';
        }
        $style = trim($spec->style) !== '' ? $spec->style : 'simple';
        if (is_array($value)) {
            return array_is_list($value)
                ? $this->serializePathArray($spec->name, $value, $style, $spec->explode)
                : $this->serializePathObject($spec->name, $value, $style, $spec->explode);
        }

        return $this->pathPrimitivePrefix($spec->name, $style) . rawurlencode((string) $value);
    }

    private function serializePathArray(string $name, array $values, string $style, bool $explode): string
    {
        $serialized = [];
        foreach ($values as $item) {
            if ($item !== null) {
                $serialized[] = rawurlencode((string) $item);
            }
        }
        if ($serialized === []) {
            return $this->pathPrefix($name, $style);
        }
        if ($style === 'matrix') {
            if ($explode) {
                return implode('', array_map(static fn($item) => ';' . $name . '=' . $item, $serialized));
            }
            return ';' . $name . '=' . implode(',', $serialized);
        }
        return $this->pathPrefix($name, $style) . implode($explode ? '.' : ',', $serialized);
    }

    private function serializePathObject(string $name, array $values, string $style, bool $explode): string
    {
        $entries = [];
        $exploded = [];
        foreach ($values as $key => $item) {
            if ($item === null) {
                continue;
            }
            $escapedKey = rawurlencode((string) $key);
            $escapedValue = rawurlencode((string) $item);
            if ($explode) {
                $exploded[] = $style === 'matrix'
                    ? ';' . $escapedKey . '=' . $escapedValue
                    : $escapedKey . '=' . $escapedValue;
            } else {
                $entries[] = $escapedKey;
                $entries[] = $escapedValue;
            }
        }
        if ($style === 'matrix') {
            return $explode ? implode('', $exploded) : ';' . $name . '=' . implode(',', $entries);
        }
        if ($explode) {
            return $this->pathPrefix($name, $style) . implode($style === 'label' ? '.' : ',', $exploded);
        }

        return $this->pathPrefix($name, $style) . implode(',', $entries);
    }

    private function pathPrefix(string $name, string $style): string
    {
        return match ($style) {
            'label' => '.',
            'matrix' => ';' . $name,
            default => '',
        };
    }

    private function pathPrimitivePrefix(string $name, string $style): string
    {
        return $style === 'matrix' ? ';' . $name . '=' : $this->pathPrefix($name, $style);
    }

    protected function appendQueryString(string $path, string $rawQueryString): string
    {
        $query = ltrim($rawQueryString, '?');
        if ($query === '') {
            return $path;
        }

        return str_contains($path, '?') ? $path . '&' . $query : $path . '?' . $query;
    }

    protected function buildQueryString(array $parameters): string
    {
        $pairs = [];
        foreach ($parameters as $parameter) {
            $this->appendSerializedParameter($pairs, $parameter);
        }

        return implode('&', $pairs);
    }

    private function appendSerializedParameter(array &$pairs, QueryParameterSpec $parameter): void
    {
        if ($parameter->value === null) {
            return;
        }

        if ($parameter->contentType !== null && trim($parameter->contentType) !== '') {
            $pairs[] = rawurlencode($parameter->name) . '=' . $this->encodeQueryValue((string) json_encode($parameter->value, JSON_UNESCAPED_SLASHES), $parameter->allowReserved);
            return;
        }

        $style = $parameter->style !== '' ? $parameter->style : 'form';
        if ($style === 'deepObject' && is_array($parameter->value)) {
            $this->appendDeepObjectParameter($pairs, $parameter->name, $parameter->value, $parameter->allowReserved);
            return;
        }
        if (is_array($parameter->value)) {
            $this->appendArrayOrObjectParameter($pairs, $parameter->name, $parameter->value, $style, $parameter->explode, $parameter->allowReserved);
            return;
        }

        $pairs[] = rawurlencode($parameter->name) . '=' . $this->encodeQueryValue((string) $parameter->value, $parameter->allowReserved);
    }

    private function appendArrayOrObjectParameter(array &$pairs, string $name, array $value, string $style, bool $explode, bool $allowReserved): void
    {
        $isList = array_is_list($value);
        $serialized = [];
        foreach ($value as $key => $item) {
            if ($item === null) {
                continue;
            }
            if (!$isList && $style === 'form' && $explode) {
                $pairs[] = rawurlencode((string) $key) . '=' . $this->encodeQueryValue((string) $item, $allowReserved);
                continue;
            }
            if ($isList) {
                $serialized[] = (string) $item;
            } else {
                $serialized[] = (string) $key;
                $serialized[] = (string) $item;
            }
        }
        if ($serialized === []) {
            return;
        }
        if ($isList && $style === 'form' && $explode) {
            foreach ($serialized as $item) {
                $pairs[] = rawurlencode($name) . '=' . $this->encodeQueryValue($item, $allowReserved);
            }
            return;
        }
        if (!(!$isList && $style === 'form' && $explode)) {
            $pairs[] = rawurlencode($name) . '=' . $this->encodeQueryValue(implode(',', $serialized), $allowReserved);
        }
    }

    private function appendDeepObjectParameter(array &$pairs, string $name, array $value, bool $allowReserved): void
    {
        foreach ($value as $key => $item) {
            if ($item !== null) {
                $pairs[] = rawurlencode($name . '[' . $key . ']') . '=' . $this->encodeQueryValue((string) $item, $allowReserved);
            }
        }
    }

    private function encodeQueryValue(string $value, bool $allowReserved): string
    {
        $encoded = rawurlencode($value);
        if (!$allowReserved) {
            return $encoded;
        }

        return strtr($encoded, [
            '%3A' => ':', '%2F' => '/', '%3F' => '?', '%23' => '#',
            '%5B' => '[', '%5D' => ']', '%40' => '@', '%21' => '!',
            '%24' => '$', '%26' => '&', '%27' => "'", '%28' => '(',
            '%29' => ')', '%2A' => '*', '%2B' => '+', '%2C' => ',',
            '%3B' => ';', '%3D' => '=',
        ]);
    }
}

final class QueryParameterSpec
{
    public function __construct(
        public string $name,
        public mixed $value,
        public string $style,
        public bool $explode,
        public bool $allowReserved,
        public ?string $contentType,
    ) {
    }
}

final class PathParameterSpec
{
    public function __construct(
        public string $name,
        public string $style,
        public bool $explode,
    ) {
    }
}

final class HeaderParameterSpec
{
    public function __construct(
        public mixed $value,
        public string $style,
        public bool $explode,
        public ?string $contentType,
    ) {
    }
}
