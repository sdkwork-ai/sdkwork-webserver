import Foundation

public struct ApiPaths {
    public static let apiPrefix = "/backend/v3/api"
    
    public static func backendPath(_ path: String = "") -> String {
        if path.isEmpty {
            return apiPrefix
        }
        if path.hasPrefix("http://") || path.hasPrefix("https://") {
            return path
        }

        let prefixRaw = apiPrefix.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedPrefix: String
        if !prefixRaw.isEmpty && prefixRaw != "/" {
            normalizedPrefix = "/" + prefixRaw.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        } else {
            normalizedPrefix = ""
        }

        let normalizedPath = path.hasPrefix("/") ? path : "/" + path
        if normalizedPrefix.isEmpty {
            return normalizedPath
        }
        if normalizedPath == normalizedPrefix || normalizedPath.hasPrefix(normalizedPrefix + "/") {
            return normalizedPath
        }
        return normalizedPrefix + normalizedPath
    }

    public static func appendQueryString(_ path: String, _ rawQueryString: String) -> String {
        let query = rawQueryString.drop(while: { $0 == "?" })
        if query.isEmpty {
            return path
        }
        return path.contains("?") ? "(path)&(query)" : "(path)?(query)"
    }
}
