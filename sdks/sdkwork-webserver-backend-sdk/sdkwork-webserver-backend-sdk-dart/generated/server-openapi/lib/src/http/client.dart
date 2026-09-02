import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'sdk_config.dart';

class HttpClient {
  HttpClient({
    required SdkConfig config,
    http.Client? innerClient,
  })  : _baseUrl = config.baseUrl,
        _timeout = Duration(milliseconds: config.timeout),
        _authToken = config.authToken,
        _accessToken = config.accessToken,
        _headers = Map<String, String>.from(config.headers),
        _client = innerClient ?? http.Client();

  final http.Client _client;
  final String _baseUrl;
  final Duration _timeout;
  final Map<String, String> _headers;
  String? _authToken;
  String? _accessToken;

  void setAuthToken(String token) {
    _authToken = token;
  }

  void setAccessToken(String token) {
    _accessToken = token;
  }

  void setHeader(String key, String value) {
    _headers[key] = value;
  }

  Future<dynamic> get(
    String path, {
    Map<String, dynamic>? params,
    Map<String, String>? headers,
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) {
    return request('GET', path, params: params, headers: headers, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);
  }

  Future<dynamic> post(
    String path, {
    dynamic body,
    Map<String, dynamic>? params,
    Map<String, String>? headers,
    String contentType = 'application/json',
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) {
    return request('POST', path, body: body, params: params, headers: headers, contentType: contentType, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);
  }

  Future<dynamic> put(
    String path, {
    dynamic body,
    Map<String, dynamic>? params,
    Map<String, String>? headers,
    String contentType = 'application/json',
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) {
    return request('PUT', path, body: body, params: params, headers: headers, contentType: contentType, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);
  }

  Future<dynamic> patch(
    String path, {
    dynamic body,
    Map<String, dynamic>? params,
    Map<String, String>? headers,
    String contentType = 'application/json',
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) {
    return request('PATCH', path, body: body, params: params, headers: headers, contentType: contentType, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);
  }

  Future<dynamic> delete(
    String path, {
    Map<String, dynamic>? params,
    Map<String, String>? headers,
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) {
    return request('DELETE', path, params: params, headers: headers, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);
  }

  Future<dynamic> request(
    String method,
    String path, {
    dynamic body,
    Map<String, dynamic>? params,
    Map<String, String>? headers,
    String contentType = 'application/json',
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) async {
    final uri = _buildUri(path, params);
    final mergedHeaders = _buildHeaders(headers, contentType: body == null ? null : contentType, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);

    http.StreamedResponse response;
    if (body != null && contentType.toLowerCase() == 'multipart/form-data') {
      response = await _sendMultipart(method, uri, body, mergedHeaders);
    } else {
      final payload = _encodeBody(body, contentType);
      final request = http.Request(method, uri)
        ..headers.addAll(mergedHeaders);
      if (payload != null) {
        request.body = payload;
      }
      response = await _client.send(request).timeout(_timeout);
    }

    final httpResponse = await http.Response.fromStream(response);
    return _decodeResponse(httpResponse);
  }

  Stream<Map<String, dynamic>> streamJson(
    String path, {
    dynamic body,
    Map<String, dynamic>? params,
    Map<String, String>? headers,
    String contentType = 'application/json',
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) async* {
    final uri = _buildUri(path, params);
    final request = http.Request('POST', uri)
      ..headers.addAll(_buildHeaders({
        'Accept': 'text/event-stream',
        ...?headers,
      }, contentType: body == null ? null : contentType, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly));
    final payload = _encodeBody(body, contentType);
    if (payload != null) {
      request.body = payload;
    }

    final response = await _client.send(request).timeout(_timeout);
    if (response.statusCode < 200 || response.statusCode >= 300) {
      final bodyText = await response.stream.bytesToString();
      throw Exception('SDKWork request failed (${response.statusCode}): $bodyText');
    }

    await for (final line in response.stream.transform(utf8.decoder).transform(const LineSplitter())) {
      final trimmed = line.trim();
      if (trimmed.isEmpty || trimmed.startsWith(':') || !trimmed.startsWith('data:')) {
        continue;
      }
      final data = trimmed.substring(5).trim();
      if (data == '[DONE]') {
        break;
      }
      final decoded = jsonDecode(data);
      if (decoded is Map<String, dynamic>) {
        yield decoded;
      } else if (decoded is Map) {
        yield Map<String, dynamic>.from(decoded);
      }
    }
  }

  void close() {
    _client.close();
  }

  Uri _buildUri(String path, Map<String, dynamic>? params) {
    final normalizedPath = path.startsWith('/') ? path : '/$path';
    final uri = Uri.parse('$_baseUrl$normalizedPath');
    if (params == null || params.isEmpty) {
      return uri;
    }

    final queryParameters = <String, String>{};
    params.forEach((key, value) {
      if (value == null) {
        return;
      }
      if (value is Iterable) {
        queryParameters[key] = value.map((item) => item.toString()).join(',');
        return;
      }
      queryParameters[key] = value.toString();
    });

    return uri.replace(queryParameters: {
      ...uri.queryParameters,
      ...queryParameters,
    });
  }

  Map<String, String> _buildHeaders(
    Map<String, String>? headers, {
    String? contentType,
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) {
    final merged = <String, String>{
      if (!skipAuth && !accessTokenOnly) ..._headers,
      ...?headers,
    };

    if (skipAuth || accessTokenOnly) {
      _stripCredentialHeaders(merged);
    }
    if (accessTokenOnly) {
      final accessToken = _accessToken?.trim();
      if (accessToken == null || accessToken.isEmpty) {
        throw StateError('access-token-only request requires Access-Token before request dispatch');
      }
      merged['Access-Token'] = accessToken;
    }

    if (contentType != null && contentType.toLowerCase() != 'multipart/form-data') {
      merged['Content-Type'] = contentType;
    }
    merged.putIfAbsent('Accept', () => 'application/json');

    if (!skipAuth && !accessTokenOnly) {
      if (_authToken != null && _authToken!.isNotEmpty) {
        merged['Authorization'] = 'Bearer $_authToken';
      }
      if (_accessToken != null && _accessToken!.isNotEmpty) {
        merged['Access-Token'] = _accessToken!;
      }
    }

    return merged;
  }

  void _stripCredentialHeaders(Map<String, String> headers) {
    const forbidden = {
      'authorization',
      'access-token',
      'x-api-key',
      'x-tenant-id',
      'x-organization-id',
      'x-platform',
      'x-user-id',
      'x-sdkwork-tenant-id',
      'x-sdkwork-organization-id',
      'x-sdkwork-user-id',
    };
    headers.removeWhere((key, _) => forbidden.contains(key.toLowerCase()));
  }

  String? _encodeBody(dynamic body, String contentType) {
    if (body == null) {
      return null;
    }

    final normalizedType = contentType.toLowerCase();
    if (normalizedType == 'application/json' || normalizedType.endsWith('+json')) {
      return jsonEncode(body);
    }
    if (normalizedType == 'application/x-www-form-urlencoded' && body is Map) {
      return body.entries
          .map((entry) => '${Uri.encodeQueryComponent(entry.key.toString())}=${Uri.encodeQueryComponent(entry.value?.toString() ?? '')}')
          .join('&');
    }
    return body.toString();
  }

  Future<http.StreamedResponse> _sendMultipart(
    String method,
    Uri uri,
    dynamic body,
    Map<String, String> headers,
  ) async {
    final request = http.MultipartRequest(method, uri);
    request.headers.addAll(headers..remove('Content-Type'));

    if (body is Map) {
      for (final entry in body.entries) {
        final key = entry.key.toString();
        final value = entry.value;
        if (value == null) {
          continue;
        }
        if (value is http.MultipartFile) {
          request.files.add(value);
          continue;
        }
        request.fields[key] = value.toString();
      }
    }

    return request.send().timeout(_timeout);
  }

  dynamic _decodeResponse(http.Response response) {
    final body = response.body;
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw Exception('SDKWork request failed (${response.statusCode}): $body');
    }
    if (body.isEmpty) {
      return null;
    }

    final contentType = response.headers['content-type']?.toLowerCase() ?? '';
    final looksLikeJson = contentType.contains('application/json')
        || contentType.contains('+json')
        || body.startsWith('{')
        || body.startsWith('[');
    if (!looksLikeJson) {
      return body;
    }

    try {
      return jsonDecode(body);
    } catch (_) {
      return body;
    }
  }
}
