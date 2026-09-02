import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;
import 'package:sdkwork_common_flutter/sdkwork_common_flutter.dart';

class HttpClient extends BaseHttpClient {
  final http.Client _rawClient = http.Client();

  HttpClient({
    required SdkConfig config,
  }) : super(config);

  @override
  Future<dynamic> request(
    String method,
    String path, {
    Map<String, dynamic>? params,
    dynamic body,
    Map<String, String>? requestHeaders,
    String? contentType,
    bool skipAuth = false,
    bool accessTokenOnly = false,
  }) async {
    final uri = _buildUri(path, params);
    final mergedHeaders = <String, String>{
      if (!skipAuth && !accessTokenOnly) ...headers,
      ...?requestHeaders,
    };
    _applyCredentialPolicy(mergedHeaders, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);

    if (body != null && _normalizeContentType(contentType).toLowerCase().startsWith('multipart/form-data')) {
      final response = await _sendMultipart(method, uri, body, mergedHeaders)
          .timeout(Duration(milliseconds: timeout));
      return _parseResponse(response);
    } else {
      final request = http.Request(method, uri)
        ..headers.addAll(_buildHeaders(mergedHeaders, contentType, body));
      final encodedBody = _encodeBody(body, contentType);
      if (encodedBody != null) {
        if (encodedBody is List<int>) {
          request.bodyBytes = encodedBody;
        } else {
          request.body = encodedBody.toString();
        }
      }
      final streamed = await _rawClient
          .send(request)
          .timeout(Duration(milliseconds: timeout));
      final response = await http.Response.fromStream(streamed);
      return _parseResponse(response);
    }
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
    final mergedHeaders = <String, String>{
      if (!skipAuth && !accessTokenOnly) ...this.headers,
      'Accept': 'text/event-stream',
      ...?headers,
    };
    _applyCredentialPolicy(mergedHeaders, skipAuth: skipAuth, accessTokenOnly: accessTokenOnly);
    final request = http.Request('POST', uri)
      ..headers.addAll(_buildHeaders(mergedHeaders, contentType, body));
    final encodedBody = _encodeBody(body, contentType);
    if (encodedBody != null) {
      if (encodedBody is List<int>) {
        request.bodyBytes = encodedBody;
      } else {
        request.body = encodedBody.toString();
      }
    }

    final response = await _rawClient
        .send(request)
        .timeout(Duration(milliseconds: timeout));
    if (response.statusCode < 200 || response.statusCode >= 300) {
      final bodyText = await response.stream.bytesToString();
      throw Exception('HTTP ${response.statusCode}: $bodyText');
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

  Uri _buildUri(String path, [Map<String, dynamic>? params]) {
    final normalizedPath = path.startsWith('/') ? path : '/$path';
    var url = Uri.parse('$baseUrl$normalizedPath');
    if (params != null && params.isNotEmpty) {
      url = url.replace(
        queryParameters: params.map((k, v) => MapEntry(k, v?.toString())),
      );
    }
    return url;
  }

  dynamic _parseResponse(http.Response response) {
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw Exception('HTTP ${response.statusCode}: ${response.body}');
    }

    if (response.body.isEmpty) {
      return null;
    }

    final contentType = response.headers['content-type'] ?? '';
    if (contentType.contains('application/json')) {
      return jsonDecode(response.body);
    }
    return response.body;
  }

  String _normalizeContentType(String? contentType) {
    if (contentType == null || contentType.trim().isEmpty) {
      return 'application/json';
    }
    return contentType.trim();
  }

  void _applyCredentialPolicy(
    Map<String, String> requestHeaders, {
    required bool skipAuth,
    required bool accessTokenOnly,
  }) {
    if (!skipAuth && !accessTokenOnly) {
      return;
    }
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
    requestHeaders.removeWhere((key, _) => forbidden.contains(key.toLowerCase()));
    if (!accessTokenOnly) {
      return;
    }
    String? accessToken;
    for (final entry in this.headers.entries) {
      if (entry.key.toLowerCase() == 'access-token' && entry.value.trim().isNotEmpty) {
        accessToken = entry.value.trim();
        break;
      }
    }
    if (accessToken == null) {
      throw StateError('access-token-only request requires Access-Token before request dispatch');
    }
    requestHeaders['Access-Token'] = accessToken;
  }

  Map<String, String> _buildHeaders(
    Map<String, String> mergedHeaders,
    String? contentType,
    dynamic body,
  ) {
    if (body == null) {
      return mergedHeaders;
    }

    final normalized = _normalizeContentType(contentType).toLowerCase();
    if (normalized.startsWith('multipart/form-data')) {
      final copied = Map<String, String>.from(mergedHeaders);
      copied.remove('Content-Type');
      return copied;
    }

    return {
      ...mergedHeaders,
      'Content-Type': _normalizeContentType(contentType),
    };
  }

  dynamic _encodeBody(dynamic body, String? contentType) {
    if (body == null) {
      return null;
    }

    final normalized = _normalizeContentType(contentType).toLowerCase();
    if (normalized.startsWith('application/x-www-form-urlencoded')) {
      final entries = _toFormEntries(body);
      return entries
          .map(
            (entry) =>
                '${Uri.encodeQueryComponent(entry.key)}=${Uri.encodeQueryComponent(entry.value)}',
          )
          .join('&');
    }

    if (normalized.contains('json')) {
      return jsonEncode(body);
    }

    if (body is String || body is List<int>) {
      return body;
    }
    return body.toString();
  }

  List<MapEntry<String, String>> _toFormEntries(dynamic body) {
    final result = <MapEntry<String, String>>[];
    void addValue(String key, dynamic value) {
      if (value == null) {
        result.add(MapEntry(key, ''));
        return;
      }
      if (value is Iterable && value is! String && value is! List<int>) {
        for (final item in value) {
          addValue(key, item);
        }
        return;
      }
      result.add(MapEntry(key, value.toString()));
    }

    if (body is Map) {
      body.forEach((key, value) {
        if (key == null) {
          return;
        }
        addValue(key.toString(), value);
      });
    } else {
      addValue('value', body);
    }

    return result;
  }

  Future<http.Response> _sendMultipart(
    String method,
    Uri uri,
    dynamic body,
    Map<String, String> mergedHeaders,
  ) async {
    final request = http.MultipartRequest(method, uri);
    request.headers.addAll(_buildHeaders(mergedHeaders, 'multipart/form-data', body));

    void addField(String key, dynamic value) {
      if (value == null) {
        request.fields[key] = '';
        return;
      }
      if (value is Iterable && value is! String && value is! List<int>) {
        for (final item in value) {
          addField(key, item);
        }
        return;
      }
      if (value is http.MultipartFile) {
        request.files.add(value);
        return;
      }
      if (value is Uint8List || value is List<int>) {
        request.files.add(http.MultipartFile.fromBytes(key, List<int>.from(value as List<int>), filename: key));
        return;
      }
      request.fields[key] = value.toString();
    }

    if (body is Map) {
      body.forEach((key, value) {
        if (key == null) {
          return;
        }
        addField(key.toString(), value);
      });
    } else if (body != null) {
      addField('value', body);
    }

    final streamed = await _rawClient
        .send(request)
        .timeout(Duration(milliseconds: timeout));
    return http.Response.fromStream(streamed);
  }
}
