package com.sdkwork.web.app.sdk

import com.sdkwork.common.core.SdkConfig
import com.sdkwork.web.app.sdk.http.HttpClient
import com.sdkwork.web.app.sdk.api.ApplicationApi
import com.sdkwork.web.app.sdk.api.DomainApi
import com.sdkwork.web.app.sdk.api.CertificateApi
import com.sdkwork.web.app.sdk.api.SourceVersionApi
import com.sdkwork.web.app.sdk.api.DeploymentApi
import com.sdkwork.web.app.sdk.api.EnvVariableApi
import com.sdkwork.web.app.sdk.api.MonitorApi

open class SdkworkAppClient {
    private val httpClient: HttpClient

    lateinit var application: ApplicationApi
    lateinit var domain: DomainApi
    lateinit var certificate: CertificateApi
    lateinit var sourceVersion: SourceVersionApi
    lateinit var deployment: DeploymentApi
    lateinit var envVariable: EnvVariableApi
    lateinit var monitor: MonitorApi

    constructor(baseUrl: String) {
        this.httpClient = HttpClient(baseUrl)
        application = ApplicationApi(httpClient)
        domain = DomainApi(httpClient)
        certificate = CertificateApi(httpClient)
        sourceVersion = SourceVersionApi(httpClient)
        deployment = DeploymentApi(httpClient)
        envVariable = EnvVariableApi(httpClient)
        monitor = MonitorApi(httpClient)
    }

    constructor(config: SdkConfig) {
        this.httpClient = HttpClient(config)
        application = ApplicationApi(httpClient)
        domain = DomainApi(httpClient)
        certificate = CertificateApi(httpClient)
        sourceVersion = SourceVersionApi(httpClient)
        deployment = DeploymentApi(httpClient)
        envVariable = EnvVariableApi(httpClient)
        monitor = MonitorApi(httpClient)
    }
    fun setAuthToken(token: String): SdkworkAppClient {
        httpClient.setAuthToken(token)
        return this
    }

    fun setAccessToken(token: String): SdkworkAppClient {
        httpClient.setAccessToken(token)
        return this
    }

    fun setHeader(key: String, value: String): SdkworkAppClient {
        httpClient.setHeader(key, value)
        return this
    }
}
