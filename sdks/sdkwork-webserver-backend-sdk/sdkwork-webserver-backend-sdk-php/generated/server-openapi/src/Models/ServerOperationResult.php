<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

final class ServerOperationResult
{
    public ?string $operationId = null;

    /** Foreground runs only; absent when the run timed out. */
    public ?int $exitCode = null;

    public ?bool $timedOut = null;

    public ?string $stdout = null;

    public ?string $stderr = null;

    /** True when stdout exceeded the capture cap and was truncated. */
    public ?bool $stdoutTruncated = null;

    /** True when stderr exceeded the capture cap and was truncated. */
    public ?bool $stderrTruncated = null;

    /** Managed runs; the recorded (or restarted) process id. */
    public ?int $pid = null;

    /** Managed runs; path of the pid-file record. */
    public ?string $pidFile = null;

    /** Managed runs; path of the append-only run log. */
    public ?string $logFile = null;

    /** Managed stops/restarts; false when no live process existed. */
    public ?bool $stopped = null;

    /** Human-readable outcome for managed runs. */
    public ?string $message = null;

    public function __construct(array $data = [])
    {
        $this->operationId = array_key_exists('operationId', $data)
            ? $data['operationId']
            : null;
        $this->exitCode = array_key_exists('exitCode', $data)
            ? $data['exitCode']
            : null;
        $this->timedOut = array_key_exists('timedOut', $data)
            ? $data['timedOut']
            : null;
        $this->stdout = array_key_exists('stdout', $data)
            ? $data['stdout']
            : null;
        $this->stderr = array_key_exists('stderr', $data)
            ? $data['stderr']
            : null;
        $this->stdoutTruncated = array_key_exists('stdoutTruncated', $data)
            ? $data['stdoutTruncated']
            : null;
        $this->stderrTruncated = array_key_exists('stderrTruncated', $data)
            ? $data['stderrTruncated']
            : null;
        $this->pid = array_key_exists('pid', $data)
            ? $data['pid']
            : null;
        $this->pidFile = array_key_exists('pidFile', $data)
            ? $data['pidFile']
            : null;
        $this->logFile = array_key_exists('logFile', $data)
            ? $data['logFile']
            : null;
        $this->stopped = array_key_exists('stopped', $data)
            ? $data['stopped']
            : null;
        $this->message = array_key_exists('message', $data)
            ? $data['message']
            : null;
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'operationId' => $this->operationId,
            'exitCode' => $this->exitCode,
            'timedOut' => $this->timedOut,
            'stdout' => $this->stdout,
            'stderr' => $this->stderr,
            'stdoutTruncated' => $this->stdoutTruncated,
            'stderrTruncated' => $this->stderrTruncated,
            'pid' => $this->pid,
            'pidFile' => $this->pidFile,
            'logFile' => $this->logFile,
            'stopped' => $this->stopped,
            'message' => $this->message,
        ];
    }
}
