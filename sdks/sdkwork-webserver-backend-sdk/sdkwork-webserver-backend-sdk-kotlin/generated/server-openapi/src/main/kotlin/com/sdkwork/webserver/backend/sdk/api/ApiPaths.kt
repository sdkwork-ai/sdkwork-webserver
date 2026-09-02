package com.sdkwork.webserver.backend.sdk.api

object ApiPaths {
    const val API_PREFIX = "/backend/v3/api"
    
    fun backendPath(path: String = ""): String {
        if (path.isEmpty()) return API_PREFIX
        if (path.startsWith("http://") || path.startsWith("https://")) return path

        var normalizedPrefix = API_PREFIX.trim()
        normalizedPrefix = if (normalizedPrefix.isNotEmpty() && normalizedPrefix != "/") {
            "/" + normalizedPrefix.trim('/')
        } else {
            ""
        }

        val normalizedPath = if (path.startsWith("/")) path else "/$path"
        if (normalizedPrefix.isEmpty()) return normalizedPath
        if (normalizedPath == normalizedPrefix || normalizedPath.startsWith("$normalizedPrefix/")) {
            return normalizedPath
        }
        return normalizedPrefix + normalizedPath
    }

    fun appendQueryString(path: String, rawQueryString: String): String {
        val query = rawQueryString.trimStart('?')
        if (query.isEmpty()) return path
        return if (path.contains("?")) "$path&$query" else "$path?$query"
    }
}
