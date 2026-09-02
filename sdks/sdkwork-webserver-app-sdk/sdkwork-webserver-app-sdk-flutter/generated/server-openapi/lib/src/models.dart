Map<String, dynamic>? _sdkworkAsMap(dynamic value) {
  if (value is Map<String, dynamic>) {
    return value;
  }
  if (value is Map) {
    return value.map((key, item) => MapEntry(key.toString(), item));
  }
  return null;
}

List<dynamic>? _sdkworkAsList(dynamic value) {
  return value is List ? value : null;
}

class ProblemDetail {
  final String type;
  final String title;
  final int status;
  final String? detail;
  final String? instance;
  final int code;
  final String traceId;
  final List<FieldError>? errors;

  ProblemDetail({
    required this.type,
    required this.title,
    required this.status,
    this.detail,
    this.instance,
    required this.code,
    required this.traceId,
    this.errors
  });

  factory ProblemDetail.fromJson(Map<String, dynamic> json) {
    return ProblemDetail(
      type: (() {
        final value = json['type']?.toString();
        if (value == null) {
          throw FormatException('ProblemDetail.type is required');
        }
        return value;
      })(),
      title: (() {
        final value = json['title']?.toString();
        if (value == null) {
          throw FormatException('ProblemDetail.title is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('ProblemDetail.status is required');
        }
        return value;
      })(),
      detail: json['detail']?.toString(),
      instance: json['instance']?.toString(),
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ProblemDetail.code is required');
        }
        return value;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ProblemDetail.traceId is required');
        }
        return value;
      })(),
      errors: (() {
        final list = _sdkworkAsList(json['errors']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : FieldError.fromJson(map);
      })())
            .whereType<FieldError>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'type': type,
      'title': title,
      'status': status,
      'detail': detail,
      'instance': instance,
      'code': code,
      'traceId': traceId,
      'errors': errors?.map((item) => item.toJson()).toList(),
    };
  }
}

class MediaChecksum {
  final String algorithm;
  final String value;

  MediaChecksum({
    required this.algorithm,
    required this.value
  });

  factory MediaChecksum.fromJson(Map<String, dynamic> json) {
    return MediaChecksum(
      algorithm: (() {
        final value = json['algorithm']?.toString();
        if (value == null) {
          throw FormatException('MediaChecksum.algorithm is required');
        }
        return value;
      })(),
      value: (() {
        final value = json['value']?.toString();
        if (value == null) {
          throw FormatException('MediaChecksum.value is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'algorithm': algorithm,
      'value': value,
    };
  }
}

class MediaResource {
  final String? id;
  final String kind;
  final String source;
  final String? url;
  final String? publicUrl;
  final String? uri;
  final String? objectBlobId;
  final String? fileName;
  final String? mimeType;
  final String? sizeBytes;
  final MediaChecksum? checksum;
  final int? width;
  final int? height;
  final double? durationSeconds;
  final String? altText;
  final String? title;
  final Map<String, dynamic>? metadata;

  MediaResource({
    this.id,
    required this.kind,
    required this.source,
    this.url,
    this.publicUrl,
    this.uri,
    this.objectBlobId,
    this.fileName,
    this.mimeType,
    this.sizeBytes,
    this.checksum,
    this.width,
    this.height,
    this.durationSeconds,
    this.altText,
    this.title,
    this.metadata
  });

  factory MediaResource.fromJson(Map<String, dynamic> json) {
    return MediaResource(
      id: json['id']?.toString(),
      kind: (() {
        final value = json['kind']?.toString();
        if (value == null) {
          throw FormatException('MediaResource.kind is required');
        }
        return value;
      })(),
      source: (() {
        final value = json['source']?.toString();
        if (value == null) {
          throw FormatException('MediaResource.source is required');
        }
        return value;
      })(),
      url: json['url']?.toString(),
      publicUrl: json['publicUrl']?.toString(),
      uri: json['uri']?.toString(),
      objectBlobId: json['objectBlobId']?.toString(),
      fileName: json['fileName']?.toString(),
      mimeType: json['mimeType']?.toString(),
      sizeBytes: json['sizeBytes']?.toString(),
      checksum: (() {
        final map = _sdkworkAsMap(json['checksum']);
        return map == null ? null : MediaChecksum.fromJson(map);
      })(),
      width: json['width'] is int ? json['width'] : null,
      height: json['height'] is int ? json['height'] : null,
      durationSeconds: json['durationSeconds'] is num ? json['durationSeconds'].toDouble() : null,
      altText: json['altText']?.toString(),
      title: json['title']?.toString(),
      metadata: _sdkworkAsMap(json['metadata'])
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'kind': kind,
      'source': source,
      'url': url,
      'publicUrl': publicUrl,
      'uri': uri,
      'objectBlobId': objectBlobId,
      'fileName': fileName,
      'mimeType': mimeType,
      'sizeBytes': sizeBytes,
      'checksum': checksum?.toJson(),
      'width': width,
      'height': height,
      'durationSeconds': durationSeconds,
      'altText': altText,
      'title': title,
      'metadata': metadata,
    };
  }
}

class PlatformTargetResponse {
  final String? id;
  final String? appId;
  final String? targetKey;
  final String? platform;
  final String? techStack;
  final List<String>? architectures;
  final String? bundleId;
  final String? packageName;
  final String? appIdValue;
  final String? bundleName;
  final String? targetStatus;
  final String? createdAt;
  final String? updatedAt;

  PlatformTargetResponse({
    this.id,
    this.appId,
    this.targetKey,
    this.platform,
    this.techStack,
    this.architectures,
    this.bundleId,
    this.packageName,
    this.appIdValue,
    this.bundleName,
    this.targetStatus,
    this.createdAt,
    this.updatedAt
  });

  factory PlatformTargetResponse.fromJson(Map<String, dynamic> json) {
    return PlatformTargetResponse(
      id: json['id']?.toString(),
      appId: json['appId']?.toString(),
      targetKey: json['targetKey']?.toString(),
      platform: json['platform']?.toString(),
      techStack: json['techStack']?.toString(),
      architectures: (() {
        final list = _sdkworkAsList(json['architectures']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      bundleId: json['bundleId']?.toString(),
      packageName: json['packageName']?.toString(),
      appIdValue: json['appIdValue']?.toString(),
      bundleName: json['bundleName']?.toString(),
      targetStatus: json['targetStatus']?.toString(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'appId': appId,
      'targetKey': targetKey,
      'platform': platform,
      'techStack': techStack,
      'architectures': architectures?.map((item) => item).toList(),
      'bundleId': bundleId,
      'packageName': packageName,
      'appIdValue': appIdValue,
      'bundleName': bundleName,
      'targetStatus': targetStatus,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class CreatePlatformTargetRequest {
  final String targetKey;
  final String platform;
  final String? techStack;
  final List<String>? architectures;
  final String? bundleId;
  final String? packageName;
  final String? appId;
  final String? bundleName;
  final List<String>? allowedChannels;

  CreatePlatformTargetRequest({
    required this.targetKey,
    required this.platform,
    this.techStack,
    this.architectures,
    this.bundleId,
    this.packageName,
    this.appId,
    this.bundleName,
    this.allowedChannels
  });

  factory CreatePlatformTargetRequest.fromJson(Map<String, dynamic> json) {
    return CreatePlatformTargetRequest(
      targetKey: (() {
        final value = json['targetKey']?.toString();
        if (value == null) {
          throw FormatException('CreatePlatformTargetRequest.targetKey is required');
        }
        return value;
      })(),
      platform: (() {
        final value = json['platform']?.toString();
        if (value == null) {
          throw FormatException('CreatePlatformTargetRequest.platform is required');
        }
        return value;
      })(),
      techStack: json['techStack']?.toString(),
      architectures: (() {
        final list = _sdkworkAsList(json['architectures']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      bundleId: json['bundleId']?.toString(),
      packageName: json['packageName']?.toString(),
      appId: json['appId']?.toString(),
      bundleName: json['bundleName']?.toString(),
      allowedChannels: (() {
        final list = _sdkworkAsList(json['allowedChannels']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'targetKey': targetKey,
      'platform': platform,
      'techStack': techStack,
      'architectures': architectures?.map((item) => item).toList(),
      'bundleId': bundleId,
      'packageName': packageName,
      'appId': appId,
      'bundleName': bundleName,
      'allowedChannels': allowedChannels?.map((item) => item).toList(),
    };
  }
}

class ApplicationStoreListing {
  final MediaResource? icon;
  final MediaResource? cover;
  final List<MediaResource>? previews;
  final String? shortDescription;
  final String? fullDescription;
  final String? releaseNotes;
  final String? category;
  final List<String>? keywords;
  final String? supportUrl;
  final String? privacyPolicyUrl;
  final String? officialWebsiteUrl;

  ApplicationStoreListing({
    this.icon,
    this.cover,
    this.previews,
    this.shortDescription,
    this.fullDescription,
    this.releaseNotes,
    this.category,
    this.keywords,
    this.supportUrl,
    this.privacyPolicyUrl,
    this.officialWebsiteUrl
  });

  factory ApplicationStoreListing.fromJson(Map<String, dynamic> json) {
    return ApplicationStoreListing(
      icon: (() {
        final map = _sdkworkAsMap(json['icon']);
        return map == null ? null : MediaResource.fromJson(map);
      })(),
      cover: (() {
        final map = _sdkworkAsMap(json['cover']);
        return map == null ? null : MediaResource.fromJson(map);
      })(),
      previews: (() {
        final list = _sdkworkAsList(json['previews']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MediaResource.fromJson(map);
      })())
            .whereType<MediaResource>()
            .toList();
      })(),
      shortDescription: json['shortDescription']?.toString(),
      fullDescription: json['fullDescription']?.toString(),
      releaseNotes: json['releaseNotes']?.toString(),
      category: json['category']?.toString(),
      keywords: (() {
        final list = _sdkworkAsList(json['keywords']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      supportUrl: json['supportUrl']?.toString(),
      privacyPolicyUrl: json['privacyPolicyUrl']?.toString(),
      officialWebsiteUrl: json['officialWebsiteUrl']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'icon': icon?.toJson(),
      'cover': cover?.toJson(),
      'previews': previews?.map((item) => item.toJson()).toList(),
      'shortDescription': shortDescription,
      'fullDescription': fullDescription,
      'releaseNotes': releaseNotes,
      'category': category,
      'keywords': keywords?.map((item) => item).toList(),
      'supportUrl': supportUrl,
      'privacyPolicyUrl': privacyPolicyUrl,
      'officialWebsiteUrl': officialWebsiteUrl,
    };
  }
}

class CreateApplicationRequest {
  final String name;
  final String? slug;
  final String? description;
  final String appKind;
  final Map<String, dynamic>? runtimeConfig;
  final ApplicationStoreListing? storeListing;

  CreateApplicationRequest({
    required this.name,
    this.slug,
    this.description,
    required this.appKind,
    this.runtimeConfig,
    this.storeListing
  });

  factory CreateApplicationRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationRequest(
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationRequest.name is required');
        }
        return value;
      })(),
      slug: json['slug']?.toString(),
      description: json['description']?.toString(),
      appKind: (() {
        final value = json['appKind']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationRequest.appKind is required');
        }
        return value;
      })(),
      runtimeConfig: _sdkworkAsMap(json['runtimeConfig']),
      storeListing: (() {
        final map = _sdkworkAsMap(json['storeListing']);
        return map == null ? null : ApplicationStoreListing.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'slug': slug,
      'description': description,
      'appKind': appKind,
      'runtimeConfig': runtimeConfig,
      'storeListing': storeListing?.toJson(),
    };
  }
}

class UpdateApplicationRequest {
  final String? name;
  final String? description;
  final Map<String, dynamic>? runtimeConfig;
  final ApplicationStoreListing? storeListing;

  UpdateApplicationRequest({
    this.name,
    this.description,
    this.runtimeConfig,
    this.storeListing
  });

  factory UpdateApplicationRequest.fromJson(Map<String, dynamic> json) {
    return UpdateApplicationRequest(
      name: json['name']?.toString(),
      description: json['description']?.toString(),
      runtimeConfig: _sdkworkAsMap(json['runtimeConfig']),
      storeListing: (() {
        final map = _sdkworkAsMap(json['storeListing']);
        return map == null ? null : ApplicationStoreListing.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'description': description,
      'runtimeConfig': runtimeConfig,
      'storeListing': storeListing?.toJson(),
    };
  }
}

class ApplicationResponse {
  final String? id;
  final String? name;
  final String? slug;
  final String? description;
  final String? siteId;
  final String? appKind;
  final int? siteType;
  final int? status;
  final Map<String, dynamic>? runtimeConfig;
  final ApplicationStoreListing? storeListing;
  final String? createdAt;
  final String? updatedAt;

  ApplicationResponse({
    this.id,
    this.name,
    this.slug,
    this.description,
    this.siteId,
    this.appKind,
    this.siteType,
    this.status,
    this.runtimeConfig,
    this.storeListing,
    this.createdAt,
    this.updatedAt
  });

  factory ApplicationResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationResponse(
      id: json['id']?.toString(),
      name: json['name']?.toString(),
      slug: json['slug']?.toString(),
      description: json['description']?.toString(),
      siteId: json['siteId']?.toString(),
      appKind: json['appKind']?.toString(),
      siteType: json['siteType'] is int ? json['siteType'] : null,
      status: json['status'] is int ? json['status'] : null,
      runtimeConfig: _sdkworkAsMap(json['runtimeConfig']),
      storeListing: (() {
        final map = _sdkworkAsMap(json['storeListing']);
        return map == null ? null : ApplicationStoreListing.fromJson(map);
      })(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'name': name,
      'slug': slug,
      'description': description,
      'siteId': siteId,
      'appKind': appKind,
      'siteType': siteType,
      'status': status,
      'runtimeConfig': runtimeConfig,
      'storeListing': storeListing?.toJson(),
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class ApplicationPage {
  final List<ApplicationResponse>? items;
  final String? total;
  final int? page;
  final int? pageSize;

  ApplicationPage({
    this.items,
    this.total,
    this.page,
    this.pageSize
  });

  factory ApplicationPage.fromJson(Map<String, dynamic> json) {
    return ApplicationPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : ApplicationResponse.fromJson(map);
      })())
            .whereType<ApplicationResponse>()
            .toList();
      })(),
      total: json['total']?.toString(),
      page: json['page'] is int ? json['page'] : null,
      pageSize: json['pageSize'] is int ? json['pageSize'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
      'page': page,
      'pageSize': pageSize,
    };
  }
}

class CreateDomainRequest {
  final String hostname;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateDomainRequest({
    required this.hostname,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateDomainRequest(
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('CreateDomainRequest.hostname is required');
        }
        return value;
      })(),
      isPrimary: json['isPrimary'] is bool ? json['isPrimary'] : null,
      sslEnabled: json['sslEnabled'] is bool ? json['sslEnabled'] : null,
      sslProvider: json['sslProvider']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'hostname': hostname,
      'isPrimary': isPrimary,
      'sslEnabled': sslEnabled,
      'sslProvider': sslProvider,
    };
  }
}

class DomainResponse {
  final String id;
  final String hostname;
  final String? applicationId;
  final String? applicationName;
  final String certificateCount;
  final bool isPrimary;
  final bool isVerified;
  final bool sslEnabled;
  final String? sslProvider;
  final int status;
  final String createdAt;

  DomainResponse({
    required this.id,
    required this.hostname,
    this.applicationId,
    this.applicationName,
    required this.certificateCount,
    required this.isPrimary,
    required this.isVerified,
    required this.sslEnabled,
    this.sslProvider,
    required this.status,
    required this.createdAt
  });

  factory DomainResponse.fromJson(Map<String, dynamic> json) {
    return DomainResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('DomainResponse.id is required');
        }
        return value;
      })(),
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('DomainResponse.hostname is required');
        }
        return value;
      })(),
      applicationId: json['applicationId']?.toString(),
      applicationName: json['applicationName']?.toString(),
      certificateCount: (() {
        final value = json['certificateCount']?.toString();
        if (value == null) {
          throw FormatException('DomainResponse.certificateCount is required');
        }
        return value;
      })(),
      isPrimary: (() {
        final value = json['isPrimary'];
        if (value is! bool) {
          throw FormatException('DomainResponse.isPrimary is required');
        }
        return value;
      })(),
      isVerified: (() {
        final value = json['isVerified'];
        if (value is! bool) {
          throw FormatException('DomainResponse.isVerified is required');
        }
        return value;
      })(),
      sslEnabled: (() {
        final value = json['sslEnabled'];
        if (value is! bool) {
          throw FormatException('DomainResponse.sslEnabled is required');
        }
        return value;
      })(),
      sslProvider: json['sslProvider']?.toString(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('DomainResponse.status is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('DomainResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'hostname': hostname,
      'applicationId': applicationId,
      'applicationName': applicationName,
      'certificateCount': certificateCount,
      'isPrimary': isPrimary,
      'isVerified': isVerified,
      'sslEnabled': sslEnabled,
      'sslProvider': sslProvider,
      'status': status,
      'createdAt': createdAt,
    };
  }
}

class DomainPage {
  final List<DomainResponse>? items;
  final String? total;

  DomainPage({
    this.items,
    this.total
  });

  factory DomainPage.fromJson(Map<String, dynamic> json) {
    return DomainPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : DomainResponse.fromJson(map);
      })())
            .whereType<DomainResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class DomainVerifyResponse {
  final bool verified;
  final String status;
  final String method;
  final String recordName;
  final String recordValue;
  final int attemptCount;
  final String expiresAt;
  final String? nextAttemptAt;
  final String? checkedAt;
  final String? failureCode;

  DomainVerifyResponse({
    required this.verified,
    required this.status,
    required this.method,
    required this.recordName,
    required this.recordValue,
    required this.attemptCount,
    required this.expiresAt,
    this.nextAttemptAt,
    this.checkedAt,
    this.failureCode
  });

  factory DomainVerifyResponse.fromJson(Map<String, dynamic> json) {
    return DomainVerifyResponse(
      verified: (() {
        final value = json['verified'];
        if (value is! bool) {
          throw FormatException('DomainVerifyResponse.verified is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('DomainVerifyResponse.status is required');
        }
        return value;
      })(),
      method: (() {
        final value = json['method']?.toString();
        if (value == null) {
          throw FormatException('DomainVerifyResponse.method is required');
        }
        return value;
      })(),
      recordName: (() {
        final value = json['recordName']?.toString();
        if (value == null) {
          throw FormatException('DomainVerifyResponse.recordName is required');
        }
        return value;
      })(),
      recordValue: (() {
        final value = json['recordValue']?.toString();
        if (value == null) {
          throw FormatException('DomainVerifyResponse.recordValue is required');
        }
        return value;
      })(),
      attemptCount: (() {
        final value = json['attemptCount'];
        if (value is! int) {
          throw FormatException('DomainVerifyResponse.attemptCount is required');
        }
        return value;
      })(),
      expiresAt: (() {
        final value = json['expiresAt']?.toString();
        if (value == null) {
          throw FormatException('DomainVerifyResponse.expiresAt is required');
        }
        return value;
      })(),
      nextAttemptAt: json['nextAttemptAt']?.toString(),
      checkedAt: json['checkedAt']?.toString(),
      failureCode: json['failureCode']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'verified': verified,
      'status': status,
      'method': method,
      'recordName': recordName,
      'recordValue': recordValue,
      'attemptCount': attemptCount,
      'expiresAt': expiresAt,
      'nextAttemptAt': nextAttemptAt,
      'checkedAt': checkedAt,
      'failureCode': failureCode,
    };
  }
}

class SourceVersionConfigSnapshot {
  final String appConfigPath;
  final String deploymentConfigPath;
  final bool appConfigDetected;
  final bool deploymentConfigDetected;

  SourceVersionConfigSnapshot({
    required this.appConfigPath,
    required this.deploymentConfigPath,
    required this.appConfigDetected,
    required this.deploymentConfigDetected
  });

  factory SourceVersionConfigSnapshot.fromJson(Map<String, dynamic> json) {
    return SourceVersionConfigSnapshot(
      appConfigPath: (() {
        final value = json['appConfigPath']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionConfigSnapshot.appConfigPath is required');
        }
        return value;
      })(),
      deploymentConfigPath: (() {
        final value = json['deploymentConfigPath']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionConfigSnapshot.deploymentConfigPath is required');
        }
        return value;
      })(),
      appConfigDetected: (() {
        final value = json['appConfigDetected'];
        if (value is! bool) {
          throw FormatException('SourceVersionConfigSnapshot.appConfigDetected is required');
        }
        return value;
      })(),
      deploymentConfigDetected: (() {
        final value = json['deploymentConfigDetected'];
        if (value is! bool) {
          throw FormatException('SourceVersionConfigSnapshot.deploymentConfigDetected is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'appConfigPath': appConfigPath,
      'deploymentConfigPath': deploymentConfigPath,
      'appConfigDetected': appConfigDetected,
      'deploymentConfigDetected': deploymentConfigDetected,
    };
  }
}

class CreateSourceVersionRequest {
  final String versionTag;
  final String sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String artifactDriveUri;
  final String artifactSize;
  final String artifactHash;
  final SourceVersionConfigSnapshot? configSnapshot;

  CreateSourceVersionRequest({
    required this.versionTag,
    required this.sourceType,
    this.sourceRef,
    this.commitHash,
    required this.artifactDriveUri,
    required this.artifactSize,
    required this.artifactHash,
    this.configSnapshot
  });

  factory CreateSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return CreateSourceVersionRequest(
      versionTag: (() {
        final value = json['versionTag']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceVersionRequest.versionTag is required');
        }
        return value;
      })(),
      sourceType: (() {
        final value = json['sourceType']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceVersionRequest.sourceType is required');
        }
        return value;
      })(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: (() {
        final value = json['artifactDriveUri']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceVersionRequest.artifactDriveUri is required');
        }
        return value;
      })(),
      artifactSize: (() {
        final value = json['artifactSize']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceVersionRequest.artifactSize is required');
        }
        return value;
      })(),
      artifactHash: (() {
        final value = json['artifactHash']?.toString();
        if (value == null) {
          throw FormatException('CreateSourceVersionRequest.artifactHash is required');
        }
        return value;
      })(),
      configSnapshot: (() {
        final map = _sdkworkAsMap(json['configSnapshot']);
        return map == null ? null : SourceVersionConfigSnapshot.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'versionTag': versionTag,
      'sourceType': sourceType,
      'sourceRef': sourceRef,
      'commitHash': commitHash,
      'artifactDriveUri': artifactDriveUri,
      'artifactSize': artifactSize,
      'artifactHash': artifactHash,
      'configSnapshot': configSnapshot?.toJson(),
    };
  }
}

class ImportGitSourceVersionRequest {
  final String versionTag;
  final String repositoryUrl;
  final String? gitRef;

  ImportGitSourceVersionRequest({
    required this.versionTag,
    required this.repositoryUrl,
    this.gitRef
  });

  factory ImportGitSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return ImportGitSourceVersionRequest(
      versionTag: (() {
        final value = json['versionTag']?.toString();
        if (value == null) {
          throw FormatException('ImportGitSourceVersionRequest.versionTag is required');
        }
        return value;
      })(),
      repositoryUrl: (() {
        final value = json['repositoryUrl']?.toString();
        if (value == null) {
          throw FormatException('ImportGitSourceVersionRequest.repositoryUrl is required');
        }
        return value;
      })(),
      gitRef: json['gitRef']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'versionTag': versionTag,
      'repositoryUrl': repositoryUrl,
      'gitRef': gitRef,
    };
  }
}

class SourceVersionResponse {
  final String id;
  final String applicationId;
  final String versionTag;
  final String sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String artifactDriveUri;
  final String artifactSize;
  final String artifactHash;
  final SourceVersionConfigSnapshot configSnapshot;
  final int status;
  final bool retained;
  final String createdAt;

  SourceVersionResponse({
    required this.id,
    required this.applicationId,
    required this.versionTag,
    required this.sourceType,
    this.sourceRef,
    this.commitHash,
    required this.artifactDriveUri,
    required this.artifactSize,
    required this.artifactHash,
    required this.configSnapshot,
    required this.status,
    required this.retained,
    required this.createdAt
  });

  factory SourceVersionResponse.fromJson(Map<String, dynamic> json) {
    return SourceVersionResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.id is required');
        }
        return value;
      })(),
      applicationId: (() {
        final value = json['applicationId']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.applicationId is required');
        }
        return value;
      })(),
      versionTag: (() {
        final value = json['versionTag']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.versionTag is required');
        }
        return value;
      })(),
      sourceType: (() {
        final value = json['sourceType']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.sourceType is required');
        }
        return value;
      })(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: (() {
        final value = json['artifactDriveUri']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.artifactDriveUri is required');
        }
        return value;
      })(),
      artifactSize: (() {
        final value = json['artifactSize']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.artifactSize is required');
        }
        return value;
      })(),
      artifactHash: (() {
        final value = json['artifactHash']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.artifactHash is required');
        }
        return value;
      })(),
      configSnapshot: (() {
        final map = _sdkworkAsMap(json['configSnapshot']);
        if (map == null) {
          throw FormatException('SourceVersionResponse.configSnapshot is required');
        }
        return SourceVersionConfigSnapshot.fromJson(map);
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('SourceVersionResponse.status is required');
        }
        return value;
      })(),
      retained: (() {
        final value = json['retained'];
        if (value is! bool) {
          throw FormatException('SourceVersionResponse.retained is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('SourceVersionResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'applicationId': applicationId,
      'versionTag': versionTag,
      'sourceType': sourceType,
      'sourceRef': sourceRef,
      'commitHash': commitHash,
      'artifactDriveUri': artifactDriveUri,
      'artifactSize': artifactSize,
      'artifactHash': artifactHash,
      'configSnapshot': configSnapshot.toJson(),
      'status': status,
      'retained': retained,
      'createdAt': createdAt,
    };
  }
}

class SourceVersionPage {
  final List<SourceVersionResponse>? items;
  final String? total;

  SourceVersionPage({
    this.items,
    this.total
  });

  factory SourceVersionPage.fromJson(Map<String, dynamic> json) {
    return SourceVersionPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : SourceVersionResponse.fromJson(map);
      })())
            .whereType<SourceVersionResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CreateDeploymentRequest {
  final String? sourceVersionId;
  final int deployType;
  final String? versionTag;
  final String? commitHash;
  final String? sourceRef;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final String? environment;

  CreateDeploymentRequest({
    this.sourceVersionId,
    required this.deployType,
    this.versionTag,
    this.commitHash,
    this.sourceRef,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash,
    this.environment
  });

  factory CreateDeploymentRequest.fromJson(Map<String, dynamic> json) {
    return CreateDeploymentRequest(
      sourceVersionId: json['sourceVersionId']?.toString(),
      deployType: (() {
        final value = json['deployType'];
        if (value is! int) {
          throw FormatException('CreateDeploymentRequest.deployType is required');
        }
        return value;
      })(),
      versionTag: json['versionTag']?.toString(),
      commitHash: json['commitHash']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
      environment: json['environment']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'sourceVersionId': sourceVersionId,
      'deployType': deployType,
      'versionTag': versionTag,
      'commitHash': commitHash,
      'sourceRef': sourceRef,
      'artifactDriveUri': artifactDriveUri,
      'artifactSize': artifactSize,
      'artifactHash': artifactHash,
      'environment': environment,
    };
  }
}

class DeploymentResponse {
  final String id;
  final String applicationId;
  final int deployType;
  final String? sourceVersionId;
  final String? versionTag;
  final String? commitHash;
  final String? sourceRef;
  final String? rollbackFromDeploymentId;
  final String environment;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final int status;
  final String? startedAt;
  final String? completedAt;
  final String? durationMs;
  final String createdAt;

  DeploymentResponse({
    required this.id,
    required this.applicationId,
    required this.deployType,
    this.sourceVersionId,
    this.versionTag,
    this.commitHash,
    this.sourceRef,
    this.rollbackFromDeploymentId,
    required this.environment,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash,
    required this.status,
    this.startedAt,
    this.completedAt,
    this.durationMs,
    required this.createdAt
  });

  factory DeploymentResponse.fromJson(Map<String, dynamic> json) {
    return DeploymentResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('DeploymentResponse.id is required');
        }
        return value;
      })(),
      applicationId: (() {
        final value = json['applicationId']?.toString();
        if (value == null) {
          throw FormatException('DeploymentResponse.applicationId is required');
        }
        return value;
      })(),
      deployType: (() {
        final value = json['deployType'];
        if (value is! int) {
          throw FormatException('DeploymentResponse.deployType is required');
        }
        return value;
      })(),
      sourceVersionId: json['sourceVersionId']?.toString(),
      versionTag: json['versionTag']?.toString(),
      commitHash: json['commitHash']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      rollbackFromDeploymentId: json['rollbackFromDeploymentId']?.toString(),
      environment: (() {
        final value = json['environment']?.toString();
        if (value == null) {
          throw FormatException('DeploymentResponse.environment is required');
        }
        return value;
      })(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('DeploymentResponse.status is required');
        }
        return value;
      })(),
      startedAt: json['startedAt']?.toString(),
      completedAt: json['completedAt']?.toString(),
      durationMs: json['durationMs']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('DeploymentResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'applicationId': applicationId,
      'deployType': deployType,
      'sourceVersionId': sourceVersionId,
      'versionTag': versionTag,
      'commitHash': commitHash,
      'sourceRef': sourceRef,
      'rollbackFromDeploymentId': rollbackFromDeploymentId,
      'environment': environment,
      'artifactDriveUri': artifactDriveUri,
      'artifactSize': artifactSize,
      'artifactHash': artifactHash,
      'status': status,
      'startedAt': startedAt,
      'completedAt': completedAt,
      'durationMs': durationMs,
      'createdAt': createdAt,
    };
  }
}

class DeploymentPage {
  final List<DeploymentResponse>? items;
  final String? total;

  DeploymentPage({
    this.items,
    this.total
  });

  factory DeploymentPage.fromJson(Map<String, dynamic> json) {
    return DeploymentPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : DeploymentResponse.fromJson(map);
      })())
            .whereType<DeploymentResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CreateEnvVariableRequest {
  final String key;
  final String value;
  final String? environment;
  final bool? isSecret;

  CreateEnvVariableRequest({
    required this.key,
    required this.value,
    this.environment,
    this.isSecret
  });

  factory CreateEnvVariableRequest.fromJson(Map<String, dynamic> json) {
    return CreateEnvVariableRequest(
      key: (() {
        final value = json['key']?.toString();
        if (value == null) {
          throw FormatException('CreateEnvVariableRequest.key is required');
        }
        return value;
      })(),
      value: (() {
        final value = json['value']?.toString();
        if (value == null) {
          throw FormatException('CreateEnvVariableRequest.value is required');
        }
        return value;
      })(),
      environment: json['environment']?.toString(),
      isSecret: json['isSecret'] is bool ? json['isSecret'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'key': key,
      'value': value,
      'environment': environment,
      'isSecret': isSecret,
    };
  }
}

class UpdateEnvVariableRequest {
  final String value;
  final bool? isSecret;

  UpdateEnvVariableRequest({
    required this.value,
    this.isSecret
  });

  factory UpdateEnvVariableRequest.fromJson(Map<String, dynamic> json) {
    return UpdateEnvVariableRequest(
      value: (() {
        final value = json['value']?.toString();
        if (value == null) {
          throw FormatException('UpdateEnvVariableRequest.value is required');
        }
        return value;
      })(),
      isSecret: json['isSecret'] is bool ? json['isSecret'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'value': value,
      'isSecret': isSecret,
    };
  }
}

class EnvVariableResponse {
  final String? id;
  final String? key;
  final String? environment;
  final bool? isSecret;
  final String? createdAt;

  EnvVariableResponse({
    this.id,
    this.key,
    this.environment,
    this.isSecret,
    this.createdAt
  });

  factory EnvVariableResponse.fromJson(Map<String, dynamic> json) {
    return EnvVariableResponse(
      id: json['id']?.toString(),
      key: json['key']?.toString(),
      environment: json['environment']?.toString(),
      isSecret: json['isSecret'] is bool ? json['isSecret'] : null,
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'key': key,
      'environment': environment,
      'isSecret': isSecret,
      'createdAt': createdAt,
    };
  }
}

class EnvVariablePage {
  final List<EnvVariableResponse>? items;
  final String? total;

  EnvVariablePage({
    this.items,
    this.total
  });

  factory EnvVariablePage.fromJson(Map<String, dynamic> json) {
    return EnvVariablePage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : EnvVariableResponse.fromJson(map);
      })())
            .whereType<EnvVariableResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class CertificateIdentifierResponse {
  final String domainId;
  final String hostname;
  final String identifierType;
  final int position;

  CertificateIdentifierResponse({
    required this.domainId,
    required this.hostname,
    required this.identifierType,
    required this.position
  });

  factory CertificateIdentifierResponse.fromJson(Map<String, dynamic> json) {
    return CertificateIdentifierResponse(
      domainId: (() {
        final value = json['domainId']?.toString();
        if (value == null) {
          throw FormatException('CertificateIdentifierResponse.domainId is required');
        }
        return value;
      })(),
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('CertificateIdentifierResponse.hostname is required');
        }
        return value;
      })(),
      identifierType: (() {
        final value = json['identifierType']?.toString();
        if (value == null) {
          throw FormatException('CertificateIdentifierResponse.identifierType is required');
        }
        return value;
      })(),
      position: (() {
        final value = json['position'];
        if (value is! int) {
          throw FormatException('CertificateIdentifierResponse.position is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'domainId': domainId,
      'hostname': hostname,
      'identifierType': identifierType,
      'position': position,
    };
  }
}

class CreateListenerCertificateBindingRequest {
  final String certificateId;
  final String? certificateVersionId;
  final int? priority;
  final bool? isDefault;

  CreateListenerCertificateBindingRequest({
    required this.certificateId,
    this.certificateVersionId,
    this.priority,
    this.isDefault
  });

  factory CreateListenerCertificateBindingRequest.fromJson(Map<String, dynamic> json) {
    return CreateListenerCertificateBindingRequest(
      certificateId: (() {
        final value = json['certificateId']?.toString();
        if (value == null) {
          throw FormatException('CreateListenerCertificateBindingRequest.certificateId is required');
        }
        return value;
      })(),
      certificateVersionId: json['certificateVersionId']?.toString(),
      priority: json['priority'] is int ? json['priority'] : null,
      isDefault: json['isDefault'] is bool ? json['isDefault'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'certificateId': certificateId,
      'certificateVersionId': certificateVersionId,
      'priority': priority,
      'isDefault': isDefault,
    };
  }
}

class ListenerCertificateBindingResponse {
  final String id;
  final String applicationId;
  final String domainId;
  final String certificateId;
  final String desiredCertificateVersionId;
  final String? currentCertificateVersionId;
  final ListenerCertificateSummaryResponse desiredCertificate;
  final ListenerCertificateSummaryResponse? currentCertificate;
  final String keyAlgorithm;
  final int priority;
  final bool isDefault;
  final String status;
  final String? activatedAt;
  final String createdAt;
  final String updatedAt;

  ListenerCertificateBindingResponse({
    required this.id,
    required this.applicationId,
    required this.domainId,
    required this.certificateId,
    required this.desiredCertificateVersionId,
    this.currentCertificateVersionId,
    required this.desiredCertificate,
    this.currentCertificate,
    required this.keyAlgorithm,
    required this.priority,
    required this.isDefault,
    required this.status,
    this.activatedAt,
    required this.createdAt,
    required this.updatedAt
  });

  factory ListenerCertificateBindingResponse.fromJson(Map<String, dynamic> json) {
    return ListenerCertificateBindingResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.id is required');
        }
        return value;
      })(),
      applicationId: (() {
        final value = json['applicationId']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.applicationId is required');
        }
        return value;
      })(),
      domainId: (() {
        final value = json['domainId']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.domainId is required');
        }
        return value;
      })(),
      certificateId: (() {
        final value = json['certificateId']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.certificateId is required');
        }
        return value;
      })(),
      desiredCertificateVersionId: (() {
        final value = json['desiredCertificateVersionId']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.desiredCertificateVersionId is required');
        }
        return value;
      })(),
      currentCertificateVersionId: json['currentCertificateVersionId']?.toString(),
      desiredCertificate: (() {
        final map = _sdkworkAsMap(json['desiredCertificate']);
        if (map == null) {
          throw FormatException('ListenerCertificateBindingResponse.desiredCertificate is required');
        }
        return ListenerCertificateSummaryResponse.fromJson(map);
      })(),
      currentCertificate: (() {
        final map = _sdkworkAsMap(json['currentCertificate']);
        return map == null ? null : ListenerCertificateSummaryResponse.fromJson(map);
      })(),
      keyAlgorithm: (() {
        final value = json['keyAlgorithm']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.keyAlgorithm is required');
        }
        return value;
      })(),
      priority: (() {
        final value = json['priority'];
        if (value is! int) {
          throw FormatException('ListenerCertificateBindingResponse.priority is required');
        }
        return value;
      })(),
      isDefault: (() {
        final value = json['isDefault'];
        if (value is! bool) {
          throw FormatException('ListenerCertificateBindingResponse.isDefault is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.status is required');
        }
        return value;
      })(),
      activatedAt: json['activatedAt']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.updatedAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'applicationId': applicationId,
      'domainId': domainId,
      'certificateId': certificateId,
      'desiredCertificateVersionId': desiredCertificateVersionId,
      'currentCertificateVersionId': currentCertificateVersionId,
      'desiredCertificate': desiredCertificate.toJson(),
      'currentCertificate': currentCertificate?.toJson(),
      'keyAlgorithm': keyAlgorithm,
      'priority': priority,
      'isDefault': isDefault,
      'status': status,
      'activatedAt': activatedAt,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class ListenerCertificateSummaryResponse {
  final String certName;
  final List<CertificateIdentifierResponse> identifiers;
  final String? issuer;
  final String? fingerprint;
  final String? notAfter;
  final String status;

  ListenerCertificateSummaryResponse({
    required this.certName,
    required this.identifiers,
    this.issuer,
    this.fingerprint,
    this.notAfter,
    required this.status
  });

  factory ListenerCertificateSummaryResponse.fromJson(Map<String, dynamic> json) {
    return ListenerCertificateSummaryResponse(
      certName: (() {
        final value = json['certName']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateSummaryResponse.certName is required');
        }
        return value;
      })(),
      identifiers: (() {
        final list = _sdkworkAsList(json['identifiers']);
        if (list == null) {
          throw FormatException('ListenerCertificateSummaryResponse.identifiers is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : CertificateIdentifierResponse.fromJson(map);
      })())
            .whereType<CertificateIdentifierResponse>()
            .toList();
      })(),
      issuer: json['issuer']?.toString(),
      fingerprint: json['fingerprint']?.toString(),
      notAfter: json['notAfter']?.toString(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateSummaryResponse.status is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'certName': certName,
      'identifiers': identifiers.map((item) => item.toJson()).toList(),
      'issuer': issuer,
      'fingerprint': fingerprint,
      'notAfter': notAfter,
      'status': status,
    };
  }
}

class CreateHealthCheckRequest {
  final int checkType;
  final String checkUrl;
  final int? checkInterval;
  final int? timeoutMs;
  final int? retryCount;

  CreateHealthCheckRequest({
    required this.checkType,
    required this.checkUrl,
    this.checkInterval,
    this.timeoutMs,
    this.retryCount
  });

  factory CreateHealthCheckRequest.fromJson(Map<String, dynamic> json) {
    return CreateHealthCheckRequest(
      checkType: (() {
        final value = json['checkType'];
        if (value is! int) {
          throw FormatException('CreateHealthCheckRequest.checkType is required');
        }
        return value;
      })(),
      checkUrl: (() {
        final value = json['checkUrl']?.toString();
        if (value == null) {
          throw FormatException('CreateHealthCheckRequest.checkUrl is required');
        }
        return value;
      })(),
      checkInterval: json['checkInterval'] is int ? json['checkInterval'] : null,
      timeoutMs: json['timeoutMs'] is int ? json['timeoutMs'] : null,
      retryCount: json['retryCount'] is int ? json['retryCount'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'checkType': checkType,
      'checkUrl': checkUrl,
      'checkInterval': checkInterval,
      'timeoutMs': timeoutMs,
      'retryCount': retryCount,
    };
  }
}

class HealthCheckResponse {
  final String id;
  final int checkType;
  final String checkUrl;
  final int checkInterval;
  final int timeoutMs;
  final int retryCount;
  final int status;
  final String createdAt;

  HealthCheckResponse({
    required this.id,
    required this.checkType,
    required this.checkUrl,
    required this.checkInterval,
    required this.timeoutMs,
    required this.retryCount,
    required this.status,
    required this.createdAt
  });

  factory HealthCheckResponse.fromJson(Map<String, dynamic> json) {
    return HealthCheckResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('HealthCheckResponse.id is required');
        }
        return value;
      })(),
      checkType: (() {
        final value = json['checkType'];
        if (value is! int) {
          throw FormatException('HealthCheckResponse.checkType is required');
        }
        return value;
      })(),
      checkUrl: (() {
        final value = json['checkUrl']?.toString();
        if (value == null) {
          throw FormatException('HealthCheckResponse.checkUrl is required');
        }
        return value;
      })(),
      checkInterval: (() {
        final value = json['checkInterval'];
        if (value is! int) {
          throw FormatException('HealthCheckResponse.checkInterval is required');
        }
        return value;
      })(),
      timeoutMs: (() {
        final value = json['timeoutMs'];
        if (value is! int) {
          throw FormatException('HealthCheckResponse.timeoutMs is required');
        }
        return value;
      })(),
      retryCount: (() {
        final value = json['retryCount'];
        if (value is! int) {
          throw FormatException('HealthCheckResponse.retryCount is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('HealthCheckResponse.status is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('HealthCheckResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'checkType': checkType,
      'checkUrl': checkUrl,
      'checkInterval': checkInterval,
      'timeoutMs': timeoutMs,
      'retryCount': retryCount,
      'status': status,
      'createdAt': createdAt,
    };
  }
}

class HealthCheckPage {
  final List<HealthCheckResponse>? items;
  final String? total;

  HealthCheckPage({
    this.items,
    this.total
  });

  factory HealthCheckPage.fromJson(Map<String, dynamic> json) {
    return HealthCheckPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : HealthCheckResponse.fromJson(map);
      })())
            .whereType<HealthCheckResponse>()
            .toList();
      })(),
      total: json['total']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item.toJson()).toList(),
      'total': total,
    };
  }
}

class SdkWorkApiResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkApiResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkApiResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkApiResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkApiResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkApiResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SdkWorkResourceData {
  final Map<String, dynamic> item;

  SdkWorkResourceData({
    required this.item
  });

  factory SdkWorkResourceData.fromJson(Map<String, dynamic> json) {
    return SdkWorkResourceData(
      item: (() {
        final map = _sdkworkAsMap(json['item']);
        if (map == null) {
          throw FormatException('SdkWorkResourceData.item is required');
        }
        return map;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'item': item,
    };
  }
}

class SdkWorkPageData {
  final List<Map<String, dynamic>> items;
  final PageInfo pageInfo;

  SdkWorkPageData({
    required this.items,
    required this.pageInfo
  });

  factory SdkWorkPageData.fromJson(Map<String, dynamic> json) {
    return SdkWorkPageData(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          throw FormatException('SdkWorkPageData.items is required');
        }
        return list
            .map((item) => _sdkworkAsMap(item))
            .whereType<Map<String, dynamic>>()
            .toList();
      })(),
      pageInfo: (() {
        final map = _sdkworkAsMap(json['pageInfo']);
        if (map == null) {
          throw FormatException('SdkWorkPageData.pageInfo is required');
        }
        return PageInfo.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items.map((item) => item).toList(),
      'pageInfo': pageInfo.toJson(),
    };
  }
}

class SdkWorkCommandData {
  final bool accepted;
  final String? resourceId;
  final String? status;

  SdkWorkCommandData({
    required this.accepted,
    this.resourceId,
    this.status
  });

  factory SdkWorkCommandData.fromJson(Map<String, dynamic> json) {
    return SdkWorkCommandData(
      accepted: (() {
        final value = json['accepted'];
        if (value is! bool) {
          throw FormatException('SdkWorkCommandData.accepted is required');
        }
        return value;
      })(),
      resourceId: json['resourceId']?.toString(),
      status: json['status']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'accepted': accepted,
      'resourceId': resourceId,
      'status': status,
    };
  }
}

class SdkWorkAsyncData {
  final bool accepted;
  final String operationId;
  final String status;
  final String? pollUrl;

  SdkWorkAsyncData({
    required this.accepted,
    required this.operationId,
    required this.status,
    this.pollUrl
  });

  factory SdkWorkAsyncData.fromJson(Map<String, dynamic> json) {
    return SdkWorkAsyncData(
      accepted: (() {
        final value = json['accepted'];
        if (value is! bool) {
          throw FormatException('SdkWorkAsyncData.accepted is required');
        }
        return value;
      })(),
      operationId: (() {
        final value = json['operationId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkAsyncData.operationId is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkAsyncData.status is required');
        }
        return value;
      })(),
      pollUrl: json['pollUrl']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'accepted': accepted,
      'operationId': operationId,
      'status': status,
      'pollUrl': pollUrl,
    };
  }
}

class PageInfo {
  final String mode;
  final int? page;
  final int? pageSize;
  final String? totalItems;
  final int? totalPages;
  final String? nextCursor;
  final bool? hasMore;

  PageInfo({
    required this.mode,
    this.page,
    this.pageSize,
    this.totalItems,
    this.totalPages,
    this.nextCursor,
    this.hasMore
  });

  factory PageInfo.fromJson(Map<String, dynamic> json) {
    return PageInfo(
      mode: (() {
        final value = json['mode']?.toString();
        if (value == null) {
          throw FormatException('PageInfo.mode is required');
        }
        return value;
      })(),
      page: json['page'] is int ? json['page'] : null,
      pageSize: json['pageSize'] is int ? json['pageSize'] : null,
      totalItems: json['totalItems']?.toString(),
      totalPages: json['totalPages'] is int ? json['totalPages'] : null,
      nextCursor: json['nextCursor']?.toString(),
      hasMore: json['hasMore'] is bool ? json['hasMore'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'mode': mode,
      'page': page,
      'pageSize': pageSize,
      'totalItems': totalItems,
      'totalPages': totalPages,
      'nextCursor': nextCursor,
      'hasMore': hasMore,
    };
  }
}

class FieldError {
  final String field;
  final String message;
  final int? code;

  FieldError({
    required this.field,
    required this.message,
    this.code
  });

  factory FieldError.fromJson(Map<String, dynamic> json) {
    return FieldError(
      field: (() {
        final value = json['field']?.toString();
        if (value == null) {
          throw FormatException('FieldError.field is required');
        }
        return value;
      })(),
      message: (() {
        final value = json['message']?.toString();
        if (value == null) {
          throw FormatException('FieldError.message is required');
        }
        return value;
      })(),
      code: json['code'] is int ? json['code'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'field': field,
      'message': message,
      'code': code,
    };
  }
}

class SdkWorkResourceResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkResourceResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkResourceResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkResourceResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkResourceResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkResourceResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SdkWorkListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkListResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkListResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class SdkWorkCommandResponse {
  final int code;
  final dynamic data;
  final String traceId;

  SdkWorkCommandResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory SdkWorkCommandResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkCommandResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('SdkWorkCommandResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('SdkWorkCommandResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsActivateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsActivateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsActivateResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsActivateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsActivateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsActivateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsActivateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsPauseResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsPauseResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsPauseResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsPauseResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsPauseResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsPauseResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsPauseResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDomainsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDomainsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDomainsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDomainsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDomainsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDomainsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDomainsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDomainsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDomainsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDomainsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDomainsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDomainsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDomainsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDomainsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDomainsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDomainsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDomainsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDomainsVerifyResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDomainsVerifyResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDomainsVerifyResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsVerifyResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDomainsVerifyResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDomainsVerifyResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDomainsVerifyResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDomainsListenerCertificateBindingsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDomainsListenerCertificateBindingsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDomainsListenerCertificateBindingsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsListenerCertificateBindingsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDomainsListenerCertificateBindingsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDomainsListenerCertificateBindingsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDomainsListenerCertificateBindingsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDomainsListenerCertificateBindingsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDomainsListenerCertificateBindingsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDomainsListenerCertificateBindingsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsListenerCertificateBindingsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDomainsListenerCertificateBindingsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDomainsListenerCertificateBindingsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDomainsListenerCertificateBindingsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsSourceVersionsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsSourceVersionsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsSourceVersionsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsSourceVersionsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsSourceVersionsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsSourceVersionsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsSourceVersionsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsSourceVersionsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsSourceVersionsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsSourceVersionsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsSourceVersionsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsSourceVersionsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsSourceVersionsGitImportCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsSourceVersionsGitImportCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsSourceVersionsGitImportCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsGitImportCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsSourceVersionsGitImportCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsSourceVersionsGitImportCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsSourceVersionsGitImportCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsSourceVersionsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsSourceVersionsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsSourceVersionsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsSourceVersionsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsSourceVersionsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsSourceVersionsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDeploymentsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDeploymentsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDeploymentsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDeploymentsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDeploymentsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDeploymentsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDeploymentsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDeploymentsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDeploymentsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDeploymentsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDeploymentsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDeploymentsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDeploymentsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDeploymentsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDeploymentsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDeploymentsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDeploymentsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDeploymentsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsDeploymentsRollbackResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsDeploymentsRollbackResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsDeploymentsRollbackResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsRollbackResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsDeploymentsRollbackResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsDeploymentsRollbackResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsDeploymentsRollbackResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsEnvVariablesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsEnvVariablesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsEnvVariablesListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsEnvVariablesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsEnvVariablesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsEnvVariablesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsEnvVariablesListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsEnvVariablesCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsEnvVariablesCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsEnvVariablesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsEnvVariablesCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsEnvVariablesCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsEnvVariablesCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsEnvVariablesCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsEnvVariablesUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsEnvVariablesUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsEnvVariablesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsEnvVariablesUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsEnvVariablesUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsEnvVariablesUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsEnvVariablesUpdateResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class DomainsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainsListResponse.fromJson(Map<String, dynamic> json) {
    return DomainsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsPlatformTargetsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsPlatformTargetsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsPlatformTargetsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsPlatformTargetsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsPlatformTargetsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsPlatformTargetsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsPlatformTargetsListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsPlatformTargetsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsPlatformTargetsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsPlatformTargetsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsPlatformTargetsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsPlatformTargetsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsPlatformTargetsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsPlatformTargetsCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsPlatformTargetsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsPlatformTargetsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsPlatformTargetsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsPlatformTargetsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsPlatformTargetsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsPlatformTargetsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsPlatformTargetsRetrieveResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsHealthChecksListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsHealthChecksListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsHealthChecksListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsHealthChecksListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsHealthChecksListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsHealthChecksListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsHealthChecksListResponse.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}

class ApplicationsHealthChecksCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ApplicationsHealthChecksCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ApplicationsHealthChecksCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsHealthChecksCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ApplicationsHealthChecksCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ApplicationsHealthChecksCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationsHealthChecksCreateResponse201.traceId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'code': code,
      'data': data,
      'traceId': traceId,
    };
  }
}
