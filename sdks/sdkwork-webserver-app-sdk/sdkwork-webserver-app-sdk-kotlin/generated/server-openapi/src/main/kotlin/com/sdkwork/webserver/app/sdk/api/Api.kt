package com.sdkwork.webserver.app.sdk.api

import com.sdkwork.webserver.app.sdk.http.HttpClient

/**
 * API modules for sdkwork-webserver-app-sdk
 */
class Api(private val client: HttpClient) {
    val application: ApplicationApi = ApplicationApi(client)
    val domain: DomainApi = DomainApi(client)
    val certificate: CertificateApi = CertificateApi(client)
    val sourceVersion: SourceVersionApi = SourceVersionApi(client)
    val deployment: DeploymentApi = DeploymentApi(client)
    val envVariable: EnvVariableApi = EnvVariableApi(client)
    val monitor: MonitorApi = MonitorApi(client)
}
