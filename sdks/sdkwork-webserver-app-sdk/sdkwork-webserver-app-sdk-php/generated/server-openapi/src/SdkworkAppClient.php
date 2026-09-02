<?php

declare(strict_types=1);

namespace SDKWork\Webserver\AppSdk;

use SDKWork\Webserver\AppSdk\Http\HttpClient;
use SDKWork\Webserver\AppSdk\Api\ApplicationApi;
use SDKWork\Webserver\AppSdk\Api\DomainApi;
use SDKWork\Webserver\AppSdk\Api\CertificateApi;
use SDKWork\Webserver\AppSdk\Api\SourceVersionApi;
use SDKWork\Webserver\AppSdk\Api\DeploymentApi;
use SDKWork\Webserver\AppSdk\Api\EnvVariableApi;
use SDKWork\Webserver\AppSdk\Api\MonitorApi;

final class SdkworkAppClient
{
    public HttpClient $http;
    public ApplicationApi $application;
    public DomainApi $domain;
    public CertificateApi $certificate;
    public SourceVersionApi $sourceVersion;
    public DeploymentApi $deployment;
    public EnvVariableApi $envVariable;
    public MonitorApi $monitor;

    public function __construct(SdkConfig $config)
    {
        $this->http = new HttpClient($config);
        $this->application = new ApplicationApi($this->http);
        $this->domain = new DomainApi($this->http);
        $this->certificate = new CertificateApi($this->http);
        $this->sourceVersion = new SourceVersionApi($this->http);
        $this->deployment = new DeploymentApi($this->http);
        $this->envVariable = new EnvVariableApi($this->http);
        $this->monitor = new MonitorApi($this->http);
    }

    public function setApiKey(string $apiKey): self
    {
        $this->http->setApiKey($apiKey);
        return $this;
    }

    public function setAuthToken(string $token): self
    {
        $this->http->setAuthToken($token);
        return $this;
    }

    public function setAccessToken(string $token): self
    {
        $this->http->setAccessToken($token);
        return $this;
    }

    public function setHeader(string $key, string $value): self
    {
        $this->http->setHeader($key, $value);
        return $this;
    }
}
