class ApiPaths {
  static const String apiPrefix = '/backend/v3/api';

  static String backendPath([String path = '']) {
    if (path.isEmpty) return apiPrefix;
    if (path.startsWith('http://') || path.startsWith('https://')) return path;

    final prefixRaw = apiPrefix.trim();
    final normalizedPrefix =
        (prefixRaw.isNotEmpty && prefixRaw != '/') ? '/${prefixRaw.replaceAll(RegExp(r'^/+|/+$'), '')}' : '';
    final normalizedPath = path.startsWith('/') ? path : '/$path';

    if (normalizedPrefix.isEmpty) return normalizedPath;
    if (normalizedPath == normalizedPrefix || normalizedPath.startsWith('$normalizedPrefix/')) {
      return normalizedPath;
    }
    return normalizedPrefix + normalizedPath;
  }

  static String appendQueryString(String path, String rawQueryString) {
    final query = rawQueryString.replaceFirst(RegExp(r'^\?+'), '');
    if (query.isEmpty) return path;
    return path.contains('?') ? '$path&$query' : '$path?$query';
  }
}
