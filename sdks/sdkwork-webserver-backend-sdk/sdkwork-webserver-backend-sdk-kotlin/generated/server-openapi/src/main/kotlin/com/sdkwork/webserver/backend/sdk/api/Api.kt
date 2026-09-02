package com.sdkwork.webserver.backend.sdk.api

import com.sdkwork.webserver.backend.sdk.http.HttpClient

/**
 * API modules for sdkwork-webserver-backend-sdk
 */
class Api(private val client: HttpClient) {
    val application: ApplicationApi = ApplicationApi(client)
    val applicationDomain: ApplicationDomainApi = ApplicationDomainApi(client)
    val certificate: CertificateApi = CertificateApi(client)
    val domain: DomainApi = DomainApi(client)
    val applicationSourceVersion: ApplicationSourceVersionApi = ApplicationSourceVersionApi(client)
    val applicationDeployment: ApplicationDeploymentApi = ApplicationDeploymentApi(client)
    val certificateDistribution: CertificateDistributionApi = CertificateDistributionApi(client)
    val nginx: NginxApi = NginxApi(client)
    val server: ServerApi = ServerApi(client)
    val serverFile: ServerFileApi = ServerFileApi(client)
    val agent: AgentApi = AgentApi(client)
    val audit: AuditApi = AuditApi(client)
}
