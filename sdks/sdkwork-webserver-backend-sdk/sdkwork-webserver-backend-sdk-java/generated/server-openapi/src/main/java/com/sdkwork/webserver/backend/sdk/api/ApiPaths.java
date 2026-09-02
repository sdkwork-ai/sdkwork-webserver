package com.sdkwork.webserver.backend.sdk.api;

public class ApiPaths {
    public static final String API_PREFIX = "/backend/v3/api";

    public static String backendPath(String path) {
        if (path == null || path.isEmpty()) {
            return API_PREFIX;
        }
        if (path.startsWith("http://") || path.startsWith("https://")) {
            return path;
        }

        String normalizedPrefix = API_PREFIX == null ? "" : API_PREFIX.trim();
        if (!normalizedPrefix.isEmpty() && !"/".equals(normalizedPrefix)) {
            normalizedPrefix = "/" + normalizedPrefix.replaceAll("^/+|/+$", "");
        } else {
            normalizedPrefix = "";
        }

        String normalizedPath = path.startsWith("/") ? path : "/" + path;
        if (normalizedPrefix.isEmpty()) {
            return normalizedPath;
        }
        if (normalizedPath.equals(normalizedPrefix) || normalizedPath.startsWith(normalizedPrefix + "/")) {
            return normalizedPath;
        }
        return normalizedPrefix + normalizedPath;
    }

    public static String appendQueryString(String path, String rawQueryString) {
        String query = rawQueryString == null ? "" : rawQueryString.replaceFirst("^\\?+", "");
        if (query.isEmpty()) {
            return path;
        }
        return path.contains("?") ? path + "&" + query : path + "?" + query;
    }
}
