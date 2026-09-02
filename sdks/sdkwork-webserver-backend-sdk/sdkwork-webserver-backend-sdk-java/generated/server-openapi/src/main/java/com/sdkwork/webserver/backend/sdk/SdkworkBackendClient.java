package com.sdkwork.webserver.backend.sdk;

import com.sdkwork.common.core.Types;
import com.sdkwork.webserver.backend.sdk.http.HttpClient;
import com.sdkwork.webserver.backend.sdk.api.ApplicationApi;
import com.sdkwork.webserver.backend.sdk.api.ApplicationDomainApi;
import com.sdkwork.webserver.backend.sdk.api.CertificateApi;
import com.sdkwork.webserver.backend.sdk.api.DomainApi;
import com.sdkwork.webserver.backend.sdk.api.ApplicationSourceVersionApi;
import com.sdkwork.webserver.backend.sdk.api.ApplicationDeploymentApi;
import com.sdkwork.webserver.backend.sdk.api.CertificateDistributionApi;
import com.sdkwork.webserver.backend.sdk.api.NginxApi;
import com.sdkwork.webserver.backend.sdk.api.ServerApi;
import com.sdkwork.webserver.backend.sdk.api.ServerFileApi;
import com.sdkwork.webserver.backend.sdk.api.AgentApi;
import com.sdkwork.webserver.backend.sdk.api.AuditApi;

public class SdkworkBackendClient {
    private final HttpClient httpClient;
    private ApplicationApi application;
    private ApplicationDomainApi applicationDomain;
    private CertificateApi certificate;
    private DomainApi domain;
    private ApplicationSourceVersionApi applicationSourceVersion;
    private ApplicationDeploymentApi applicationDeployment;
    private CertificateDistributionApi certificateDistribution;
    private NginxApi nginx;
    private ServerApi server;
    private ServerFileApi serverFile;
    private AgentApi agent;
    private AuditApi audit;

    public SdkworkBackendClient(String baseUrl) {
        this.httpClient = new HttpClient(baseUrl);
        this.application = new ApplicationApi(httpClient);
        this.applicationDomain = new ApplicationDomainApi(httpClient);
        this.certificate = new CertificateApi(httpClient);
        this.domain = new DomainApi(httpClient);
        this.applicationSourceVersion = new ApplicationSourceVersionApi(httpClient);
        this.applicationDeployment = new ApplicationDeploymentApi(httpClient);
        this.certificateDistribution = new CertificateDistributionApi(httpClient);
        this.nginx = new NginxApi(httpClient);
        this.server = new ServerApi(httpClient);
        this.serverFile = new ServerFileApi(httpClient);
        this.agent = new AgentApi(httpClient);
        this.audit = new AuditApi(httpClient);
    }

    public SdkworkBackendClient(Types.SdkConfig config) {
        this.httpClient = new HttpClient(config);
        this.application = new ApplicationApi(httpClient);
        this.applicationDomain = new ApplicationDomainApi(httpClient);
        this.certificate = new CertificateApi(httpClient);
        this.domain = new DomainApi(httpClient);
        this.applicationSourceVersion = new ApplicationSourceVersionApi(httpClient);
        this.applicationDeployment = new ApplicationDeploymentApi(httpClient);
        this.certificateDistribution = new CertificateDistributionApi(httpClient);
        this.nginx = new NginxApi(httpClient);
        this.server = new ServerApi(httpClient);
        this.serverFile = new ServerFileApi(httpClient);
        this.agent = new AgentApi(httpClient);
        this.audit = new AuditApi(httpClient);
    }

    public ApplicationApi getApplication() {
        return this.application;
    }

    public ApplicationDomainApi getApplicationDomain() {
        return this.applicationDomain;
    }

    public CertificateApi getCertificate() {
        return this.certificate;
    }

    public DomainApi getDomain() {
        return this.domain;
    }

    public ApplicationSourceVersionApi getApplicationSourceVersion() {
        return this.applicationSourceVersion;
    }

    public ApplicationDeploymentApi getApplicationDeployment() {
        return this.applicationDeployment;
    }

    public CertificateDistributionApi getCertificateDistribution() {
        return this.certificateDistribution;
    }

    public NginxApi getNginx() {
        return this.nginx;
    }

    public ServerApi getServer() {
        return this.server;
    }

    public ServerFileApi getServerFile() {
        return this.serverFile;
    }

    public AgentApi getAgent() {
        return this.agent;
    }

    public AuditApi getAudit() {
        return this.audit;
    }
    public SdkworkBackendClient setAuthToken(String token) {
        httpClient.setAuthToken(token);
        return this;
    }

    public SdkworkBackendClient setAccessToken(String token) {
        httpClient.setAccessToken(token);
        return this;
    }

    public SdkworkBackendClient setHeader(String key, String value) {
        httpClient.setHeader(key, value);
        return this;
    }

    public HttpClient getHttpClient() {
        return httpClient;
    }
}
