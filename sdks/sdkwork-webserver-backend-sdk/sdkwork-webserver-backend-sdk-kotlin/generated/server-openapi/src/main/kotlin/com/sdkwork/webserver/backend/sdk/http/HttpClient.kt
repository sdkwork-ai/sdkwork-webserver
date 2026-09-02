package com.sdkwork.webserver.backend.sdk.http

import com.fasterxml.jackson.core.type.TypeReference
import com.sdkwork.common.core.SdkConfig
import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.module.kotlin.registerKotlinModule
import okhttp3.Headers
import okhttp3.FormBody
import okhttp3.HttpUrl
import okhttp3.MediaType
import okhttp3.MultipartBody
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import java.io.BufferedReader
import java.io.InputStreamReader
import java.nio.charset.StandardCharsets
import java.util.concurrent.TimeUnit

class HttpClient(
    private val baseUrl: String,
    timeoutMs: Int = 30000,
    defaultHeaders: Map<String, String> = emptyMap()
) {

    private val client: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(timeoutMs.toLong(), TimeUnit.MILLISECONDS)
        .readTimeout(timeoutMs.toLong(), TimeUnit.MILLISECONDS)
        .writeTimeout(timeoutMs.toLong(), TimeUnit.MILLISECONDS)
        .build()
    private val mapper = ObjectMapper().registerKotlinModule()
    private val headers = mutableMapOf<String, String>()

    constructor(config: SdkConfig) : this(
        baseUrl = config.baseUrl.trimEnd('/'),
        timeoutMs = config.timeout ?: 30000,
        defaultHeaders = config.headers ?: emptyMap()
    )

    init {
        headers.putAll(defaultHeaders)
    }
    fun setAuthToken(token: String) {
        headers["Authorization"] = "Bearer $token"
    }
    fun setAccessToken(token: String) {
        headers["Access-Token"] = token
    }

    fun setHeader(key: String, value: String) {
        headers[key] = value
    }

    private fun buildUrl(path: String, params: Map<String, Any>? = null): HttpUrl {
        val urlBuilder = HttpUrl.parse(baseUrl + path)!!.newBuilder()
        params?.forEach { (key, value) ->
            urlBuilder.addQueryParameter(key, value.toString())
        }
        return urlBuilder.build()
    }

    private fun mergeHeaders(
        requestHeaders: Map<String, String>? = null,
        skipAuth: Boolean = false,
        accessTokenOnly: Boolean = false
    ): Headers {
        val merged = if (!skipAuth && !accessTokenOnly) headers.toMutableMap() else mutableMapOf()
        requestHeaders?.forEach { (key, value) ->
            if (key.isNotBlank() && ((!skipAuth && !accessTokenOnly) || !isCredentialHeader(key))) {
                merged[key] = value
            }
        }
        if (accessTokenOnly) {
            val accessToken = headers.entries
                .firstOrNull { it.key.equals("Access-Token", ignoreCase = true) && it.value.isNotBlank() }
                ?.value
                ?.trim()
                ?: throw IllegalStateException(
                    "access-token-only request requires Access-Token before request dispatch"
                )
            merged["Access-Token"] = accessToken
        }
        return Headers.of(merged)
    }

    private fun isCredentialHeader(key: String): Boolean = when (key.lowercase()) {
        "authorization", "access-token", "x-api-key", "x-tenant-id",
        "x-organization-id", "x-platform", "x-user-id", "x-sdkwork-tenant-id",
        "x-sdkwork-organization-id", "x-sdkwork-user-id" -> true
        else -> false
    }

    private fun createJsonBody(body: Any?): RequestBody {
        val payload = body ?: emptyMap<String, Any>()
        return RequestBody.create(
            MediaType.parse("application/json"),
            mapper.writeValueAsString(payload)
        )
    }

    private fun createMultipartBody(body: Any?): RequestBody {
        if (body is RequestBody) {
            return body
        }

        val builder = MultipartBody.Builder().setType(MultipartBody.FORM)
        when (body) {
            is Map<*, *> -> {
                body.forEach { (key, value) ->
                    if (key == null) return@forEach
                    val partName = key.toString()
                    when (value) {
                        null -> builder.addFormDataPart(partName, "")
                        is ByteArray -> builder.addFormDataPart(
                            partName,
                            partName,
                            RequestBody.create(MediaType.parse("application/octet-stream"), value)
                        )
                        is Iterable<*> -> value.forEach { item ->
                            builder.addFormDataPart(partName, item?.toString() ?: "")
                        }
                        else -> builder.addFormDataPart(partName, value.toString())
                    }
                }
            }
            null -> {}
            else -> builder.addFormDataPart("value", body.toString())
        }
        return builder.build()
    }

    private fun createFormBody(body: Any?): RequestBody {
        if (body is RequestBody) {
            return body
        }
        val builder = FormBody.Builder()
        when (body) {
            is Map<*, *> -> {
                body.forEach { (key, value) ->
                    if (key == null) return@forEach
                    val fieldName = key.toString()
                    when (value) {
                        null -> builder.add(fieldName, "")
                        is Iterable<*> -> value.forEach { item ->
                            builder.add(fieldName, item?.toString() ?: "")
                        }
                        else -> builder.add(fieldName, value.toString())
                    }
                }
            }
            null -> {}
            else -> builder.add("value", body.toString())
        }
        return builder.build()
    }

    private fun createRequestBody(body: Any?, contentType: String?): RequestBody {
        val normalized = contentType?.lowercase() ?: "application/json"
        return when {
            normalized.startsWith("multipart/form-data") -> createMultipartBody(body)
            normalized.startsWith("application/x-www-form-urlencoded") -> createFormBody(body)
            body is RequestBody -> body
            else -> createJsonBody(body)
        }
    }

    private fun parseResponse(responseBody: String?): Any? {
        if (responseBody.isNullOrBlank()) {
            return null
        }
        return mapper.readValue(responseBody, Any::class.java)
    }

    fun <T> convertValue(value: Any?, typeReference: TypeReference<T>): T? {
        if (value == null) {
            return null
        }
        return mapper.convertValue(value, typeReference)
    }

    suspend fun request(
        method: String,
        path: String,
        body: Any? = null,
        params: Map<String, Any>? = null,
        requestHeaders: Map<String, String>? = null,
        contentType: String? = null,
        skipAuth: Boolean = false,
        accessTokenOnly: Boolean = false
    ): Any? {
        val requestBuilder = Request.Builder()
            .url(buildUrl(path, params))
            .headers(mergeHeaders(requestHeaders, skipAuth, accessTokenOnly))

        val requestBody = if (body == null) null else createRequestBody(body, contentType)
        val request = requestBuilder
            .method(method, requestBody)
            .build()

        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw RuntimeException("HTTP ${response.code()}: ${response.body()?.string() ?: ""}")
            }
            return parseResponse(response.body()?.string())
        }
    }

    fun <T> stream(
        method: String,
        path: String,
        body: Any? = null,
        params: Map<String, Any>? = null,
        requestHeaders: Map<String, String>? = null,
        contentType: String? = null,
        typeReference: TypeReference<T>,
        skipAuth: Boolean = false,
        accessTokenOnly: Boolean = false
    ): Sequence<T> {
        return sequence {
            val requestBody = if (body == null) null else createRequestBody(body, contentType)
            val request = Request.Builder()
                .url(buildUrl(path, params))
                .headers(mergeHeaders(requestHeaders, skipAuth, accessTokenOnly))
                .addHeader("Accept", "text/event-stream")
                .method(method, requestBody)
                .build()

            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) {
                    throw RuntimeException("HTTP ${response.code()}: ${response.body()?.string() ?: ""}")
                }
                BufferedReader(InputStreamReader(response.body()?.byteStream(), StandardCharsets.UTF_8)).use { reader ->
                    while (true) {
                        val line = reader.readLine() ?: break
                        if (line.isBlank() || line.startsWith(":") || !line.startsWith("data:")) {
                            continue
                        }
                        val data = line.removePrefix("data:").trim()
                        if (data == "[DONE]") {
                            break
                        }
                        yield(mapper.readValue(data, typeReference))
                    }
                }
            }
        }
    }

    suspend fun get(path: String, params: Map<String, Any>? = null, requestHeaders: Map<String, String>? = null): Any? {
        val request = Request.Builder()
            .url(buildUrl(path, params))
            .headers(mergeHeaders(requestHeaders))
            .get()
            .build()

        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw RuntimeException("HTTP ${response.code()}: ${response.body()?.string() ?: ""}")
            }
            return parseResponse(response.body()?.string())
        }
    }

    suspend fun post(
        path: String,
        body: Any? = null,
        params: Map<String, Any>? = null,
        requestHeaders: Map<String, String>? = null,
        contentType: String? = null
    ): Any? {
        val requestBody = createRequestBody(body, contentType)
        val request = Request.Builder()
            .url(buildUrl(path, params))
            .headers(mergeHeaders(requestHeaders))
            .post(requestBody)
            .build()

        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw RuntimeException("HTTP ${response.code()}: ${response.body()?.string() ?: ""}")
            }
            return parseResponse(response.body()?.string())
        }
    }

    suspend fun put(
        path: String,
        body: Any? = null,
        params: Map<String, Any>? = null,
        requestHeaders: Map<String, String>? = null,
        contentType: String? = null
    ): Any? {
        val requestBody = createRequestBody(body, contentType)
        val request = Request.Builder()
            .url(buildUrl(path, params))
            .headers(mergeHeaders(requestHeaders))
            .put(requestBody)
            .build()

        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw RuntimeException("HTTP ${response.code()}: ${response.body()?.string() ?: ""}")
            }
            return parseResponse(response.body()?.string())
        }
    }

    suspend fun delete(path: String, params: Map<String, Any>? = null, requestHeaders: Map<String, String>? = null): Any? {
        val request = Request.Builder()
            .url(buildUrl(path, params))
            .headers(mergeHeaders(requestHeaders))
            .delete()
            .build()

        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw RuntimeException("HTTP ${response.code()}: ${response.body()?.string() ?: ""}")
            }
            return parseResponse(response.body()?.string())
        }
    }

    suspend fun patch(
        path: String,
        body: Any? = null,
        params: Map<String, Any>? = null,
        requestHeaders: Map<String, String>? = null,
        contentType: String? = null
    ): Any? {
        val requestBody = createRequestBody(body, contentType)
        val request = Request.Builder()
            .url(buildUrl(path, params))
            .headers(mergeHeaders(requestHeaders))
            .patch(requestBody)
            .build()

        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                throw RuntimeException("HTTP ${response.code()}: ${response.body()?.string() ?: ""}")
            }
            return parseResponse(response.body()?.string())
        }
    }
}
