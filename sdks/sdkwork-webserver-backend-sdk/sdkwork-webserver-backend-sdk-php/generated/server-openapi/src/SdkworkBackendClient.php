<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk;

use SDKWork\Webserver\BackendSdk\Http\HttpClient;
use SDKWork\Webserver\BackendSdk\Api\ApplicationApi;
use SDKWork\Webserver\BackendSdk\Api\ApplicationDomainApi;
use SDKWork\Webserver\BackendSdk\Api\CertificateApi;
use SDKWork\Webserver\BackendSdk\Api\DomainApi;
use SDKWork\Webserver\BackendSdk\Api\ApplicationSourceVersionApi;
use SDKWork\Webserver\BackendSdk\Api\ApplicationDeploymentApi;
use SDKWork\Webserver\BackendSdk\Api\CertificateDistributionApi;
use SDKWork\Webserver\BackendSdk\Api\NginxApi;
use SDKWork\Webserver\BackendSdk\Api\ServerApi;
use SDKWork\Webserver\BackendSdk\Api\ServerFileApi;
use SDKWork\Webserver\BackendSdk\Api\AgentApi;
use SDKWork\Webserver\BackendSdk\Api\AuditApi;

final class SdkworkBackendClient
{
    public HttpClient $http;
    public ApplicationApi $application;
    public ApplicationDomainApi $applicationDomain;
    public CertificateApi $certificate;
    public DomainApi $domain;
    public ApplicationSourceVersionApi $applicationSourceVersion;
    public ApplicationDeploymentApi $applicationDeployment;
    public CertificateDistributionApi $certificateDistribution;
    public NginxApi $nginx;
    public ServerApi $server;
    public ServerFileApi $serverFile;
    public AgentApi $agent;
    public AuditApi $audit;

    public function __construct(SdkConfig $config)
    {
        $this->http = new HttpClient($config);
        $this->application = new ApplicationApi($this->http);
        $this->applicationDomain = new ApplicationDomainApi($this->http);
        $this->certificate = new CertificateApi($this->http);
        $this->domain = new DomainApi($this->http);
        $this->applicationSourceVersion = new ApplicationSourceVersionApi($this->http);
        $this->applicationDeployment = new ApplicationDeploymentApi($this->http);
        $this->certificateDistribution = new CertificateDistributionApi($this->http);
        $this->nginx = new NginxApi($this->http);
        $this->server = new ServerApi($this->http);
        $this->serverFile = new ServerFileApi($this->http);
        $this->agent = new AgentApi($this->http);
        $this->audit = new AuditApi($this->http);
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
