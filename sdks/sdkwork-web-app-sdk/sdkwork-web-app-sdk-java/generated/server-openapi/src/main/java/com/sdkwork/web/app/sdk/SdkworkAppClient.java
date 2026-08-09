package com.sdkwork.web.app.sdk;

import com.sdkwork.common.core.Types;
import com.sdkwork.web.app.sdk.http.HttpClient;
import com.sdkwork.web.app.sdk.api.ApplicationApi;
import com.sdkwork.web.app.sdk.api.DomainApi;
import com.sdkwork.web.app.sdk.api.CertificateApi;
import com.sdkwork.web.app.sdk.api.SourceVersionApi;
import com.sdkwork.web.app.sdk.api.DeploymentApi;
import com.sdkwork.web.app.sdk.api.EnvVariableApi;
import com.sdkwork.web.app.sdk.api.MonitorApi;

public class SdkworkAppClient {
    private final HttpClient httpClient;
    private ApplicationApi application;
    private DomainApi domain;
    private CertificateApi certificate;
    private SourceVersionApi sourceVersion;
    private DeploymentApi deployment;
    private EnvVariableApi envVariable;
    private MonitorApi monitor;

    public SdkworkAppClient(String baseUrl) {
        this.httpClient = new HttpClient(baseUrl);
        this.application = new ApplicationApi(httpClient);
        this.domain = new DomainApi(httpClient);
        this.certificate = new CertificateApi(httpClient);
        this.sourceVersion = new SourceVersionApi(httpClient);
        this.deployment = new DeploymentApi(httpClient);
        this.envVariable = new EnvVariableApi(httpClient);
        this.monitor = new MonitorApi(httpClient);
    }

    public SdkworkAppClient(Types.SdkConfig config) {
        this.httpClient = new HttpClient(config);
        this.application = new ApplicationApi(httpClient);
        this.domain = new DomainApi(httpClient);
        this.certificate = new CertificateApi(httpClient);
        this.sourceVersion = new SourceVersionApi(httpClient);
        this.deployment = new DeploymentApi(httpClient);
        this.envVariable = new EnvVariableApi(httpClient);
        this.monitor = new MonitorApi(httpClient);
    }

    public ApplicationApi getApplication() {
        return this.application;
    }

    public DomainApi getDomain() {
        return this.domain;
    }

    public CertificateApi getCertificate() {
        return this.certificate;
    }

    public SourceVersionApi getSourceVersion() {
        return this.sourceVersion;
    }

    public DeploymentApi getDeployment() {
        return this.deployment;
    }

    public EnvVariableApi getEnvVariable() {
        return this.envVariable;
    }

    public MonitorApi getMonitor() {
        return this.monitor;
    }
    public SdkworkAppClient setAuthToken(String token) {
        httpClient.setAuthToken(token);
        return this;
    }

    public SdkworkAppClient setAccessToken(String token) {
        httpClient.setAccessToken(token);
        return this;
    }

    public SdkworkAppClient setHeader(String key, String value) {
        httpClient.setHeader(key, value);
        return this;
    }

    public HttpClient getHttpClient() {
        return httpClient;
    }
}
