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

class CreateNginxConfigRequest {
  final int configType;
  final String configName;
  final String configContent;
  final String siteId;

  CreateNginxConfigRequest({
    required this.configType,
    required this.configName,
    required this.configContent,
    required this.siteId
  });

  factory CreateNginxConfigRequest.fromJson(Map<String, dynamic> json) {
    return CreateNginxConfigRequest(
      configType: (() {
        final value = json['configType'];
        if (value is! int) {
          throw FormatException('CreateNginxConfigRequest.configType is required');
        }
        return value;
      })(),
      configName: (() {
        final value = json['configName']?.toString();
        if (value == null) {
          throw FormatException('CreateNginxConfigRequest.configName is required');
        }
        return value;
      })(),
      configContent: (() {
        final value = json['configContent']?.toString();
        if (value == null) {
          throw FormatException('CreateNginxConfigRequest.configContent is required');
        }
        return value;
      })(),
      siteId: (() {
        final value = json['siteId']?.toString();
        if (value == null) {
          throw FormatException('CreateNginxConfigRequest.siteId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'configType': configType,
      'configName': configName,
      'configContent': configContent,
      'siteId': siteId,
    };
  }
}

class UpdateNginxConfigRequest {
  final String? configContent;
  final String? configName;

  UpdateNginxConfigRequest({
    this.configContent,
    this.configName
  });

  factory UpdateNginxConfigRequest.fromJson(Map<String, dynamic> json) {
    return UpdateNginxConfigRequest(
      configContent: json['configContent']?.toString(),
      configName: json['configName']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'configContent': configContent,
      'configName': configName,
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
  final String id;
  final String name;
  final String slug;
  final String? description;
  final String? appKind;
  final int siteType;
  final int status;
  final Map<String, dynamic>? runtimeConfig;
  final ApplicationStoreListing? storeListing;
  final String createdAt;
  final String updatedAt;

  ApplicationResponse({
    required this.id,
    required this.name,
    required this.slug,
    this.description,
    this.appKind,
    required this.siteType,
    required this.status,
    this.runtimeConfig,
    this.storeListing,
    required this.createdAt,
    required this.updatedAt
  });

  factory ApplicationResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ApplicationResponse.id is required');
        }
        return value;
      })(),
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('ApplicationResponse.name is required');
        }
        return value;
      })(),
      slug: (() {
        final value = json['slug']?.toString();
        if (value == null) {
          throw FormatException('ApplicationResponse.slug is required');
        }
        return value;
      })(),
      description: json['description']?.toString(),
      appKind: json['appKind']?.toString(),
      siteType: (() {
        final value = json['siteType'];
        if (value is! int) {
          throw FormatException('ApplicationResponse.siteType is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('ApplicationResponse.status is required');
        }
        return value;
      })(),
      runtimeConfig: _sdkworkAsMap(json['runtimeConfig']),
      storeListing: (() {
        final map = _sdkworkAsMap(json['storeListing']);
        return map == null ? null : ApplicationStoreListing.fromJson(map);
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('ApplicationResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('ApplicationResponse.updatedAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'name': name,
      'slug': slug,
      'description': description,
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

class CreateApplicationDomainRequest {
  final String hostname;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateApplicationDomainRequest({
    required this.hostname,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateApplicationDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationDomainRequest(
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationDomainRequest.hostname is required');
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

class CreateManagedDomainRequest {
  final String hostname;
  final String? applicationId;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateManagedDomainRequest({
    required this.hostname,
    this.applicationId,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateManagedDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateManagedDomainRequest(
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('CreateManagedDomainRequest.hostname is required');
        }
        return value;
      })(),
      applicationId: json['applicationId']?.toString(),
      isPrimary: json['isPrimary'] is bool ? json['isPrimary'] : null,
      sslEnabled: json['sslEnabled'] is bool ? json['sslEnabled'] : null,
      sslProvider: json['sslProvider']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'hostname': hostname,
      'applicationId': applicationId,
      'isPrimary': isPrimary,
      'sslEnabled': sslEnabled,
      'sslProvider': sslProvider,
    };
  }
}

class CreateRootDomainRequest {
  final String hostname;

  CreateRootDomainRequest({
    required this.hostname
  });

  factory CreateRootDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateRootDomainRequest(
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('CreateRootDomainRequest.hostname is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'hostname': hostname,
    };
  }
}

class CreateRootDomainHostnameRequest {
  final String recordName;
  final String? applicationId;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateRootDomainHostnameRequest({
    required this.recordName,
    this.applicationId,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateRootDomainHostnameRequest.fromJson(Map<String, dynamic> json) {
    return CreateRootDomainHostnameRequest(
      recordName: (() {
        final value = json['recordName']?.toString();
        if (value == null) {
          throw FormatException('CreateRootDomainHostnameRequest.recordName is required');
        }
        return value;
      })(),
      applicationId: json['applicationId']?.toString(),
      isPrimary: json['isPrimary'] is bool ? json['isPrimary'] : null,
      sslEnabled: json['sslEnabled'] is bool ? json['sslEnabled'] : null,
      sslProvider: json['sslProvider']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'recordName': recordName,
      'applicationId': applicationId,
      'isPrimary': isPrimary,
      'sslEnabled': sslEnabled,
      'sslProvider': sslProvider,
    };
  }
}

class RootDomainResponse {
  final String id;
  final String hostname;
  final int status;
  final String subdomainCount;
  final String boundSubdomainCount;
  final String verifiedSubdomainCount;
  final String httpsSubdomainCount;
  final String activeDeploymentCount;
  final String createdAt;
  final String updatedAt;

  RootDomainResponse({
    required this.id,
    required this.hostname,
    required this.status,
    required this.subdomainCount,
    required this.boundSubdomainCount,
    required this.verifiedSubdomainCount,
    required this.httpsSubdomainCount,
    required this.activeDeploymentCount,
    required this.createdAt,
    required this.updatedAt
  });

  factory RootDomainResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.id is required');
        }
        return value;
      })(),
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.hostname is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('RootDomainResponse.status is required');
        }
        return value;
      })(),
      subdomainCount: (() {
        final value = json['subdomainCount']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.subdomainCount is required');
        }
        return value;
      })(),
      boundSubdomainCount: (() {
        final value = json['boundSubdomainCount']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.boundSubdomainCount is required');
        }
        return value;
      })(),
      verifiedSubdomainCount: (() {
        final value = json['verifiedSubdomainCount']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.verifiedSubdomainCount is required');
        }
        return value;
      })(),
      httpsSubdomainCount: (() {
        final value = json['httpsSubdomainCount']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.httpsSubdomainCount is required');
        }
        return value;
      })(),
      activeDeploymentCount: (() {
        final value = json['activeDeploymentCount']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.activeDeploymentCount is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('RootDomainResponse.updatedAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'hostname': hostname,
      'status': status,
      'subdomainCount': subdomainCount,
      'boundSubdomainCount': boundSubdomainCount,
      'verifiedSubdomainCount': verifiedSubdomainCount,
      'httpsSubdomainCount': httpsSubdomainCount,
      'activeDeploymentCount': activeDeploymentCount,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class DomainDeploymentResponse {
  final String id;
  final int status;
  final String environment;
  final String? versionTag;
  final String? completedAt;
  final String createdAt;

  DomainDeploymentResponse({
    required this.id,
    required this.status,
    required this.environment,
    this.versionTag,
    this.completedAt,
    required this.createdAt
  });

  factory DomainDeploymentResponse.fromJson(Map<String, dynamic> json) {
    return DomainDeploymentResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('DomainDeploymentResponse.id is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('DomainDeploymentResponse.status is required');
        }
        return value;
      })(),
      environment: (() {
        final value = json['environment']?.toString();
        if (value == null) {
          throw FormatException('DomainDeploymentResponse.environment is required');
        }
        return value;
      })(),
      versionTag: json['versionTag']?.toString(),
      completedAt: json['completedAt']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('DomainDeploymentResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'status': status,
      'environment': environment,
      'versionTag': versionTag,
      'completedAt': completedAt,
      'createdAt': createdAt,
    };
  }
}

class UpdateDomainApplicationBindingRequest {
  final String applicationId;
  final bool? isPrimary;

  UpdateDomainApplicationBindingRequest({
    required this.applicationId,
    this.isPrimary
  });

  factory UpdateDomainApplicationBindingRequest.fromJson(Map<String, dynamic> json) {
    return UpdateDomainApplicationBindingRequest(
      applicationId: (() {
        final value = json['applicationId']?.toString();
        if (value == null) {
          throw FormatException('UpdateDomainApplicationBindingRequest.applicationId is required');
        }
        return value;
      })(),
      isPrimary: json['isPrimary'] is bool ? json['isPrimary'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'applicationId': applicationId,
      'isPrimary': isPrimary,
    };
  }
}

class ApplicationDomainResponse {
  final String id;
  final String hostname;
  final String? rootDomainId;
  final String? recordName;
  final String? applicationId;
  final String? applicationName;
  final String certificateCount;
  final bool isPrimary;
  final bool isVerified;
  final bool sslEnabled;
  final String? sslProvider;
  final int status;
  final DomainDeploymentResponse? latestDeployment;
  final String createdAt;
  final String? updatedAt;

  ApplicationDomainResponse({
    required this.id,
    required this.hostname,
    this.rootDomainId,
    this.recordName,
    this.applicationId,
    this.applicationName,
    required this.certificateCount,
    required this.isPrimary,
    required this.isVerified,
    required this.sslEnabled,
    this.sslProvider,
    required this.status,
    this.latestDeployment,
    required this.createdAt,
    this.updatedAt
  });

  factory ApplicationDomainResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationDomainResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDomainResponse.id is required');
        }
        return value;
      })(),
      hostname: (() {
        final value = json['hostname']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDomainResponse.hostname is required');
        }
        return value;
      })(),
      rootDomainId: json['rootDomainId']?.toString(),
      recordName: json['recordName']?.toString(),
      applicationId: json['applicationId']?.toString(),
      applicationName: json['applicationName']?.toString(),
      certificateCount: (() {
        final value = json['certificateCount']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDomainResponse.certificateCount is required');
        }
        return value;
      })(),
      isPrimary: (() {
        final value = json['isPrimary'];
        if (value is! bool) {
          throw FormatException('ApplicationDomainResponse.isPrimary is required');
        }
        return value;
      })(),
      isVerified: (() {
        final value = json['isVerified'];
        if (value is! bool) {
          throw FormatException('ApplicationDomainResponse.isVerified is required');
        }
        return value;
      })(),
      sslEnabled: (() {
        final value = json['sslEnabled'];
        if (value is! bool) {
          throw FormatException('ApplicationDomainResponse.sslEnabled is required');
        }
        return value;
      })(),
      sslProvider: json['sslProvider']?.toString(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('ApplicationDomainResponse.status is required');
        }
        return value;
      })(),
      latestDeployment: (() {
        final map = _sdkworkAsMap(json['latestDeployment']);
        return map == null ? null : DomainDeploymentResponse.fromJson(map);
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDomainResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'hostname': hostname,
      'rootDomainId': rootDomainId,
      'recordName': recordName,
      'applicationId': applicationId,
      'applicationName': applicationName,
      'certificateCount': certificateCount,
      'isPrimary': isPrimary,
      'isVerified': isVerified,
      'sslEnabled': sslEnabled,
      'sslProvider': sslProvider,
      'status': status,
      'latestDeployment': latestDeployment?.toJson(),
      'createdAt': createdAt,
      'updatedAt': updatedAt,
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

class ApplicationSourceVersionConfigSnapshot {
  final String appConfigPath;
  final String deploymentConfigPath;
  final bool appConfigDetected;
  final bool deploymentConfigDetected;

  ApplicationSourceVersionConfigSnapshot({
    required this.appConfigPath,
    required this.deploymentConfigPath,
    required this.appConfigDetected,
    required this.deploymentConfigDetected
  });

  factory ApplicationSourceVersionConfigSnapshot.fromJson(Map<String, dynamic> json) {
    return ApplicationSourceVersionConfigSnapshot(
      appConfigPath: (() {
        final value = json['appConfigPath']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionConfigSnapshot.appConfigPath is required');
        }
        return value;
      })(),
      deploymentConfigPath: (() {
        final value = json['deploymentConfigPath']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionConfigSnapshot.deploymentConfigPath is required');
        }
        return value;
      })(),
      appConfigDetected: (() {
        final value = json['appConfigDetected'];
        if (value is! bool) {
          throw FormatException('ApplicationSourceVersionConfigSnapshot.appConfigDetected is required');
        }
        return value;
      })(),
      deploymentConfigDetected: (() {
        final value = json['deploymentConfigDetected'];
        if (value is! bool) {
          throw FormatException('ApplicationSourceVersionConfigSnapshot.deploymentConfigDetected is required');
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

class CreateApplicationSourceVersionRequest {
  final String versionTag;
  final String sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String artifactDriveUri;
  final String artifactSize;
  final String artifactHash;
  final ApplicationSourceVersionConfigSnapshot? configSnapshot;

  CreateApplicationSourceVersionRequest({
    required this.versionTag,
    required this.sourceType,
    this.sourceRef,
    this.commitHash,
    required this.artifactDriveUri,
    required this.artifactSize,
    required this.artifactHash,
    this.configSnapshot
  });

  factory CreateApplicationSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationSourceVersionRequest(
      versionTag: (() {
        final value = json['versionTag']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationSourceVersionRequest.versionTag is required');
        }
        return value;
      })(),
      sourceType: (() {
        final value = json['sourceType']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationSourceVersionRequest.sourceType is required');
        }
        return value;
      })(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: (() {
        final value = json['artifactDriveUri']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationSourceVersionRequest.artifactDriveUri is required');
        }
        return value;
      })(),
      artifactSize: (() {
        final value = json['artifactSize']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationSourceVersionRequest.artifactSize is required');
        }
        return value;
      })(),
      artifactHash: (() {
        final value = json['artifactHash']?.toString();
        if (value == null) {
          throw FormatException('CreateApplicationSourceVersionRequest.artifactHash is required');
        }
        return value;
      })(),
      configSnapshot: (() {
        final map = _sdkworkAsMap(json['configSnapshot']);
        return map == null ? null : ApplicationSourceVersionConfigSnapshot.fromJson(map);
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

class ImportApplicationGitSourceVersionRequest {
  final String versionTag;
  final String repositoryUrl;
  final String? gitRef;

  ImportApplicationGitSourceVersionRequest({
    required this.versionTag,
    required this.repositoryUrl,
    this.gitRef
  });

  factory ImportApplicationGitSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return ImportApplicationGitSourceVersionRequest(
      versionTag: (() {
        final value = json['versionTag']?.toString();
        if (value == null) {
          throw FormatException('ImportApplicationGitSourceVersionRequest.versionTag is required');
        }
        return value;
      })(),
      repositoryUrl: (() {
        final value = json['repositoryUrl']?.toString();
        if (value == null) {
          throw FormatException('ImportApplicationGitSourceVersionRequest.repositoryUrl is required');
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

class ApplicationSourceVersionResponse {
  final String id;
  final String siteId;
  final String versionTag;
  final String sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String artifactDriveUri;
  final String artifactSize;
  final String artifactHash;
  final ApplicationSourceVersionConfigSnapshot configSnapshot;
  final int status;
  final bool retained;
  final String createdAt;

  ApplicationSourceVersionResponse({
    required this.id,
    required this.siteId,
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

  factory ApplicationSourceVersionResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationSourceVersionResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.id is required');
        }
        return value;
      })(),
      siteId: (() {
        final value = json['siteId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.siteId is required');
        }
        return value;
      })(),
      versionTag: (() {
        final value = json['versionTag']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.versionTag is required');
        }
        return value;
      })(),
      sourceType: (() {
        final value = json['sourceType']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.sourceType is required');
        }
        return value;
      })(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: (() {
        final value = json['artifactDriveUri']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.artifactDriveUri is required');
        }
        return value;
      })(),
      artifactSize: (() {
        final value = json['artifactSize']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.artifactSize is required');
        }
        return value;
      })(),
      artifactHash: (() {
        final value = json['artifactHash']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.artifactHash is required');
        }
        return value;
      })(),
      configSnapshot: (() {
        final map = _sdkworkAsMap(json['configSnapshot']);
        if (map == null) {
          throw FormatException('ApplicationSourceVersionResponse.configSnapshot is required');
        }
        return ApplicationSourceVersionConfigSnapshot.fromJson(map);
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('ApplicationSourceVersionResponse.status is required');
        }
        return value;
      })(),
      retained: (() {
        final value = json['retained'];
        if (value is! bool) {
          throw FormatException('ApplicationSourceVersionResponse.retained is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('ApplicationSourceVersionResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'siteId': siteId,
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

class CreateApplicationDeploymentRequest {
  final String? sourceVersionId;
  final int? deployType;
  final String? environment;
  final String? versionTag;
  final String? commitHash;
  final String? sourceRef;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;

  CreateApplicationDeploymentRequest({
    this.sourceVersionId,
    this.deployType,
    this.environment,
    this.versionTag,
    this.commitHash,
    this.sourceRef,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash
  });

  factory CreateApplicationDeploymentRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationDeploymentRequest(
      sourceVersionId: json['sourceVersionId']?.toString(),
      deployType: json['deployType'] is int ? json['deployType'] : null,
      environment: json['environment']?.toString(),
      versionTag: json['versionTag']?.toString(),
      commitHash: json['commitHash']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'sourceVersionId': sourceVersionId,
      'deployType': deployType,
      'environment': environment,
      'versionTag': versionTag,
      'commitHash': commitHash,
      'sourceRef': sourceRef,
      'artifactDriveUri': artifactDriveUri,
      'artifactSize': artifactSize,
      'artifactHash': artifactHash,
    };
  }
}

class ApplicationDeploymentResponse {
  final String id;
  final String siteId;
  final String? sourceVersionId;
  final int status;
  final int deployType;
  final String environment;
  final String? versionTag;
  final String? commitHash;
  final String? sourceRef;
  final String? rollbackFromDeploymentId;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final String? startedAt;
  final String? completedAt;
  final String? durationMs;
  final String createdAt;

  ApplicationDeploymentResponse({
    required this.id,
    required this.siteId,
    this.sourceVersionId,
    required this.status,
    required this.deployType,
    required this.environment,
    this.versionTag,
    this.commitHash,
    this.sourceRef,
    this.rollbackFromDeploymentId,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash,
    this.startedAt,
    this.completedAt,
    this.durationMs,
    required this.createdAt
  });

  factory ApplicationDeploymentResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationDeploymentResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDeploymentResponse.id is required');
        }
        return value;
      })(),
      siteId: (() {
        final value = json['siteId']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDeploymentResponse.siteId is required');
        }
        return value;
      })(),
      sourceVersionId: json['sourceVersionId']?.toString(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('ApplicationDeploymentResponse.status is required');
        }
        return value;
      })(),
      deployType: (() {
        final value = json['deployType'];
        if (value is! int) {
          throw FormatException('ApplicationDeploymentResponse.deployType is required');
        }
        return value;
      })(),
      environment: (() {
        final value = json['environment']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDeploymentResponse.environment is required');
        }
        return value;
      })(),
      versionTag: json['versionTag']?.toString(),
      commitHash: json['commitHash']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      rollbackFromDeploymentId: json['rollbackFromDeploymentId']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
      startedAt: json['startedAt']?.toString(),
      completedAt: json['completedAt']?.toString(),
      durationMs: json['durationMs']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('ApplicationDeploymentResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'siteId': siteId,
      'sourceVersionId': sourceVersionId,
      'status': status,
      'deployType': deployType,
      'environment': environment,
      'versionTag': versionTag,
      'commitHash': commitHash,
      'sourceRef': sourceRef,
      'rollbackFromDeploymentId': rollbackFromDeploymentId,
      'artifactDriveUri': artifactDriveUri,
      'artifactSize': artifactSize,
      'artifactHash': artifactHash,
      'startedAt': startedAt,
      'completedAt': completedAt,
      'durationMs': durationMs,
      'createdAt': createdAt,
    };
  }
}

class IssueCertificateRequest {
  final List<String> domainIds;
  final int certType;
  final String? keyAlgorithm;
  final bool? autoRenew;

  IssueCertificateRequest({
    required this.domainIds,
    required this.certType,
    this.keyAlgorithm,
    this.autoRenew
  });

  factory IssueCertificateRequest.fromJson(Map<String, dynamic> json) {
    return IssueCertificateRequest(
      domainIds: (() {
        final list = _sdkworkAsList(json['domainIds']);
        if (list == null) {
          throw FormatException('IssueCertificateRequest.domainIds is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      certType: (() {
        final value = json['certType'];
        if (value is! int) {
          throw FormatException('IssueCertificateRequest.certType is required');
        }
        return value;
      })(),
      keyAlgorithm: json['keyAlgorithm']?.toString(),
      autoRenew: json['autoRenew'] is bool ? json['autoRenew'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'domainIds': domainIds.map((item) => item).toList(),
      'certType': certType,
      'keyAlgorithm': keyAlgorithm,
      'autoRenew': autoRenew,
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

class UpdateCertificateRequest {
  final bool autoRenew;

  UpdateCertificateRequest({
    required this.autoRenew
  });

  factory UpdateCertificateRequest.fromJson(Map<String, dynamic> json) {
    return UpdateCertificateRequest(
      autoRenew: (() {
        final value = json['autoRenew'];
        if (value is! bool) {
          throw FormatException('UpdateCertificateRequest.autoRenew is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'autoRenew': autoRenew,
    };
  }
}

class CertificateResponse {
  final String id;
  final String certName;
  final List<CertificateIdentifierResponse> identifiers;
  final int? certType;
  final String? issuer;
  final String? fingerprint;
  final String keyAlgorithm;
  final String? notBefore;
  final String? notAfter;
  final bool? autoRenew;
  final String? renewalStatus;
  final String status;
  final String createdAt;

  CertificateResponse({
    required this.id,
    required this.certName,
    required this.identifiers,
    this.certType,
    this.issuer,
    this.fingerprint,
    required this.keyAlgorithm,
    this.notBefore,
    this.notAfter,
    this.autoRenew,
    this.renewalStatus,
    required this.status,
    required this.createdAt
  });

  factory CertificateResponse.fromJson(Map<String, dynamic> json) {
    return CertificateResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.id is required');
        }
        return value;
      })(),
      certName: (() {
        final value = json['certName']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.certName is required');
        }
        return value;
      })(),
      identifiers: (() {
        final list = _sdkworkAsList(json['identifiers']);
        if (list == null) {
          throw FormatException('CertificateResponse.identifiers is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : CertificateIdentifierResponse.fromJson(map);
      })())
            .whereType<CertificateIdentifierResponse>()
            .toList();
      })(),
      certType: json['certType'] is int ? json['certType'] : null,
      issuer: json['issuer']?.toString(),
      fingerprint: json['fingerprint']?.toString(),
      keyAlgorithm: (() {
        final value = json['keyAlgorithm']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.keyAlgorithm is required');
        }
        return value;
      })(),
      notBefore: json['notBefore']?.toString(),
      notAfter: json['notAfter']?.toString(),
      autoRenew: json['autoRenew'] is bool ? json['autoRenew'] : null,
      renewalStatus: json['renewalStatus']?.toString(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.status is required');
        }
        return value;
      })(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'certName': certName,
      'identifiers': identifiers.map((item) => item.toJson()).toList(),
      'certType': certType,
      'issuer': issuer,
      'fingerprint': fingerprint,
      'keyAlgorithm': keyAlgorithm,
      'notBefore': notBefore,
      'notAfter': notAfter,
      'autoRenew': autoRenew,
      'renewalStatus': renewalStatus,
      'status': status,
      'createdAt': createdAt,
    };
  }
}

class RevokeCertificateRequest {
  final String reason;

  RevokeCertificateRequest({
    required this.reason
  });

  factory RevokeCertificateRequest.fromJson(Map<String, dynamic> json) {
    return RevokeCertificateRequest(
      reason: (() {
        final value = json['reason']?.toString();
        if (value == null) {
          throw FormatException('RevokeCertificateRequest.reason is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'reason': reason,
    };
  }
}

class CertificateOperationResponse {
  final String id;
  final String certificateId;
  final String operationType;
  final String status;
  final int attemptCount;
  final int maxAttempts;
  final String nextAttemptAt;
  final String? failureCode;
  final String createdAt;
  final String updatedAt;
  final String? completedAt;

  CertificateOperationResponse({
    required this.id,
    required this.certificateId,
    required this.operationType,
    required this.status,
    required this.attemptCount,
    required this.maxAttempts,
    required this.nextAttemptAt,
    this.failureCode,
    required this.createdAt,
    required this.updatedAt,
    this.completedAt
  });

  factory CertificateOperationResponse.fromJson(Map<String, dynamic> json) {
    return CertificateOperationResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('CertificateOperationResponse.id is required');
        }
        return value;
      })(),
      certificateId: (() {
        final value = json['certificateId']?.toString();
        if (value == null) {
          throw FormatException('CertificateOperationResponse.certificateId is required');
        }
        return value;
      })(),
      operationType: (() {
        final value = json['operationType']?.toString();
        if (value == null) {
          throw FormatException('CertificateOperationResponse.operationType is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('CertificateOperationResponse.status is required');
        }
        return value;
      })(),
      attemptCount: (() {
        final value = json['attemptCount'];
        if (value is! int) {
          throw FormatException('CertificateOperationResponse.attemptCount is required');
        }
        return value;
      })(),
      maxAttempts: (() {
        final value = json['maxAttempts'];
        if (value is! int) {
          throw FormatException('CertificateOperationResponse.maxAttempts is required');
        }
        return value;
      })(),
      nextAttemptAt: (() {
        final value = json['nextAttemptAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateOperationResponse.nextAttemptAt is required');
        }
        return value;
      })(),
      failureCode: json['failureCode']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateOperationResponse.createdAt is required');
        }
        return value;
      })(),
      updatedAt: (() {
        final value = json['updatedAt']?.toString();
        if (value == null) {
          throw FormatException('CertificateOperationResponse.updatedAt is required');
        }
        return value;
      })(),
      completedAt: json['completedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'certificateId': certificateId,
      'operationType': operationType,
      'status': status,
      'attemptCount': attemptCount,
      'maxAttempts': maxAttempts,
      'nextAttemptAt': nextAttemptAt,
      'failureCode': failureCode,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
      'completedAt': completedAt,
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
  final String siteId;
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
    required this.siteId,
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
      siteId: (() {
        final value = json['siteId']?.toString();
        if (value == null) {
          throw FormatException('ListenerCertificateBindingResponse.siteId is required');
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
      'siteId': siteId,
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

class CertificateDistributionResponse {
  final String serverId;
  final String serverName;
  final String host;
  final String desiredSyncVersion;
  final String? appliedSyncVersion;
  final String status;
  final String? lastHeartbeatAt;

  CertificateDistributionResponse({
    required this.serverId,
    required this.serverName,
    required this.host,
    required this.desiredSyncVersion,
    this.appliedSyncVersion,
    required this.status,
    this.lastHeartbeatAt
  });

  factory CertificateDistributionResponse.fromJson(Map<String, dynamic> json) {
    return CertificateDistributionResponse(
      serverId: (() {
        final value = json['serverId']?.toString();
        if (value == null) {
          throw FormatException('CertificateDistributionResponse.serverId is required');
        }
        return value;
      })(),
      serverName: (() {
        final value = json['serverName']?.toString();
        if (value == null) {
          throw FormatException('CertificateDistributionResponse.serverName is required');
        }
        return value;
      })(),
      host: (() {
        final value = json['host']?.toString();
        if (value == null) {
          throw FormatException('CertificateDistributionResponse.host is required');
        }
        return value;
      })(),
      desiredSyncVersion: (() {
        final value = json['desiredSyncVersion']?.toString();
        if (value == null) {
          throw FormatException('CertificateDistributionResponse.desiredSyncVersion is required');
        }
        return value;
      })(),
      appliedSyncVersion: json['appliedSyncVersion']?.toString(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('CertificateDistributionResponse.status is required');
        }
        return value;
      })(),
      lastHeartbeatAt: json['lastHeartbeatAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'serverId': serverId,
      'serverName': serverName,
      'host': host,
      'desiredSyncVersion': desiredSyncVersion,
      'appliedSyncVersion': appliedSyncVersion,
      'status': status,
      'lastHeartbeatAt': lastHeartbeatAt,
    };
  }
}

class NginxConfigResponse {
  final String? id;
  final int? configType;
  final String? configName;
  final String? configContent;
  final String? configHash;
  final bool? isActive;
  final int? versionNo;
  final String? deployedAt;
  final int? status;
  final String? createdAt;
  final String? updatedAt;

  NginxConfigResponse({
    this.id,
    this.configType,
    this.configName,
    this.configContent,
    this.configHash,
    this.isActive,
    this.versionNo,
    this.deployedAt,
    this.status,
    this.createdAt,
    this.updatedAt
  });

  factory NginxConfigResponse.fromJson(Map<String, dynamic> json) {
    return NginxConfigResponse(
      id: json['id']?.toString(),
      configType: json['configType'] is int ? json['configType'] : null,
      configName: json['configName']?.toString(),
      configContent: json['configContent']?.toString(),
      configHash: json['configHash']?.toString(),
      isActive: json['isActive'] is bool ? json['isActive'] : null,
      versionNo: json['versionNo'] is int ? json['versionNo'] : null,
      deployedAt: json['deployedAt']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'configType': configType,
      'configName': configName,
      'configContent': configContent,
      'configHash': configHash,
      'isActive': isActive,
      'versionNo': versionNo,
      'deployedAt': deployedAt,
      'status': status,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class NginxConfigPage {
  final List<NginxConfigResponse>? items;
  final String? total;

  NginxConfigPage({
    this.items,
    this.total
  });

  factory NginxConfigPage.fromJson(Map<String, dynamic> json) {
    return NginxConfigPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : NginxConfigResponse.fromJson(map);
      })())
            .whereType<NginxConfigResponse>()
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

class NginxValidateResponse {
  final bool? valid;
  final List<Map<String, dynamic>>? errors;

  NginxValidateResponse({
    this.valid,
    this.errors
  });

  factory NginxValidateResponse.fromJson(Map<String, dynamic> json) {
    return NginxValidateResponse(
      valid: json['valid'] is bool ? json['valid'] : null,
      errors: (() {
        final list = _sdkworkAsList(json['errors']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => _sdkworkAsMap(item))
            .whereType<Map<String, dynamic>>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'valid': valid,
      'errors': errors?.map((item) => item).toList(),
    };
  }
}

class NginxDeployResponse {
  final bool? success;
  final String? configId;
  final String? deployedAt;
  final Map<String, dynamic>? reloadResult;

  NginxDeployResponse({
    this.success,
    this.configId,
    this.deployedAt,
    this.reloadResult
  });

  factory NginxDeployResponse.fromJson(Map<String, dynamic> json) {
    return NginxDeployResponse(
      success: json['success'] is bool ? json['success'] : null,
      configId: json['configId']?.toString(),
      deployedAt: json['deployedAt']?.toString(),
      reloadResult: _sdkworkAsMap(json['reloadResult'])
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'success': success,
      'configId': configId,
      'deployedAt': deployedAt,
      'reloadResult': reloadResult,
    };
  }
}

class NginxReloadResponse {
  final bool? success;
  final String? message;
  final String? timestamp;

  NginxReloadResponse({
    this.success,
    this.message,
    this.timestamp
  });

  factory NginxReloadResponse.fromJson(Map<String, dynamic> json) {
    return NginxReloadResponse(
      success: json['success'] is bool ? json['success'] : null,
      message: json['message']?.toString(),
      timestamp: json['timestamp']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'success': success,
      'message': message,
      'timestamp': timestamp,
    };
  }
}

class NginxStatusResponse {
  final bool? running;
  final String? version;
  final int? pid;
  final int? activeConnections;
  final String? configPath;
  final String? uptime;

  NginxStatusResponse({
    this.running,
    this.version,
    this.pid,
    this.activeConnections,
    this.configPath,
    this.uptime
  });

  factory NginxStatusResponse.fromJson(Map<String, dynamic> json) {
    return NginxStatusResponse(
      running: json['running'] is bool ? json['running'] : null,
      version: json['version']?.toString(),
      pid: json['pid'] is int ? json['pid'] : null,
      activeConnections: json['activeConnections'] is int ? json['activeConnections'] : null,
      configPath: json['configPath']?.toString(),
      uptime: json['uptime']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'running': running,
      'version': version,
      'pid': pid,
      'activeConnections': activeConnections,
      'configPath': configPath,
      'uptime': uptime,
    };
  }
}

class CreateServerRequest {
  final String name;
  final String host;
  final String tenantScopeHash;
  final int sshPort;

  CreateServerRequest({
    required this.name,
    required this.host,
    required this.tenantScopeHash,
    required this.sshPort
  });

  factory CreateServerRequest.fromJson(Map<String, dynamic> json) {
    return CreateServerRequest(
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('CreateServerRequest.name is required');
        }
        return value;
      })(),
      host: (() {
        final value = json['host']?.toString();
        if (value == null) {
          throw FormatException('CreateServerRequest.host is required');
        }
        return value;
      })(),
      tenantScopeHash: (() {
        final value = json['tenantScopeHash']?.toString();
        if (value == null) {
          throw FormatException('CreateServerRequest.tenantScopeHash is required');
        }
        return value;
      })(),
      sshPort: (() {
        final value = json['sshPort'];
        if (value is! int) {
          throw FormatException('CreateServerRequest.sshPort is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'host': host,
      'tenantScopeHash': tenantScopeHash,
      'sshPort': sshPort,
    };
  }
}

class ServerResponse {
  final String id;
  final String name;
  final String host;
  final String tenantScopeHash;
  final int sshPort;
  final int status;
  final String? lastHeartbeatAt;
  final String createdAt;

  ServerResponse({
    required this.id,
    required this.name,
    required this.host,
    required this.tenantScopeHash,
    required this.sshPort,
    required this.status,
    this.lastHeartbeatAt,
    required this.createdAt
  });

  factory ServerResponse.fromJson(Map<String, dynamic> json) {
    return ServerResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ServerResponse.id is required');
        }
        return value;
      })(),
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('ServerResponse.name is required');
        }
        return value;
      })(),
      host: (() {
        final value = json['host']?.toString();
        if (value == null) {
          throw FormatException('ServerResponse.host is required');
        }
        return value;
      })(),
      tenantScopeHash: (() {
        final value = json['tenantScopeHash']?.toString();
        if (value == null) {
          throw FormatException('ServerResponse.tenantScopeHash is required');
        }
        return value;
      })(),
      sshPort: (() {
        final value = json['sshPort'];
        if (value is! int) {
          throw FormatException('ServerResponse.sshPort is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('ServerResponse.status is required');
        }
        return value;
      })(),
      lastHeartbeatAt: json['lastHeartbeatAt']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('ServerResponse.createdAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'name': name,
      'host': host,
      'tenantScopeHash': tenantScopeHash,
      'sshPort': sshPort,
      'status': status,
      'lastHeartbeatAt': lastHeartbeatAt,
      'createdAt': createdAt,
    };
  }
}

class CreateServerResponse {
  final String id;
  final String name;
  final String host;
  final String tenantScopeHash;
  final int sshPort;
  final int status;
  final String? lastHeartbeatAt;
  final String createdAt;
  final String agentToken;

  CreateServerResponse({
    required this.id,
    required this.name,
    required this.host,
    required this.tenantScopeHash,
    required this.sshPort,
    required this.status,
    this.lastHeartbeatAt,
    required this.createdAt,
    required this.agentToken
  });

  factory CreateServerResponse.fromJson(Map<String, dynamic> json) {
    return CreateServerResponse(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('CreateServerResponse.id is required');
        }
        return value;
      })(),
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('CreateServerResponse.name is required');
        }
        return value;
      })(),
      host: (() {
        final value = json['host']?.toString();
        if (value == null) {
          throw FormatException('CreateServerResponse.host is required');
        }
        return value;
      })(),
      tenantScopeHash: (() {
        final value = json['tenantScopeHash']?.toString();
        if (value == null) {
          throw FormatException('CreateServerResponse.tenantScopeHash is required');
        }
        return value;
      })(),
      sshPort: (() {
        final value = json['sshPort'];
        if (value is! int) {
          throw FormatException('CreateServerResponse.sshPort is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('CreateServerResponse.status is required');
        }
        return value;
      })(),
      lastHeartbeatAt: json['lastHeartbeatAt']?.toString(),
      createdAt: (() {
        final value = json['createdAt']?.toString();
        if (value == null) {
          throw FormatException('CreateServerResponse.createdAt is required');
        }
        return value;
      })(),
      agentToken: (() {
        final value = json['agentToken']?.toString();
        if (value == null) {
          throw FormatException('CreateServerResponse.agentToken is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'name': name,
      'host': host,
      'tenantScopeHash': tenantScopeHash,
      'sshPort': sshPort,
      'status': status,
      'lastHeartbeatAt': lastHeartbeatAt,
      'createdAt': createdAt,
      'agentToken': agentToken,
    };
  }
}

class ServerFilesNode {
  final String id;
  final String name;
  final String host;
  final int sshPort;
  final String status;
  final String filesystemRoot;
  final String? region;

  ServerFilesNode({
    required this.id,
    required this.name,
    required this.host,
    required this.sshPort,
    required this.status,
    required this.filesystemRoot,
    this.region
  });

  factory ServerFilesNode.fromJson(Map<String, dynamic> json) {
    return ServerFilesNode(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNode.id is required');
        }
        return value;
      })(),
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNode.name is required');
        }
        return value;
      })(),
      host: (() {
        final value = json['host']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNode.host is required');
        }
        return value;
      })(),
      sshPort: (() {
        final value = json['sshPort'];
        if (value is! int) {
          throw FormatException('ServerFilesNode.sshPort is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNode.status is required');
        }
        return value;
      })(),
      filesystemRoot: (() {
        final value = json['filesystemRoot']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNode.filesystemRoot is required');
        }
        return value;
      })(),
      region: json['region']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'name': name,
      'host': host,
      'sshPort': sshPort,
      'status': status,
      'filesystemRoot': filesystemRoot,
      'region': region,
    };
  }
}

class ServerDirectoryListing {
  final String nodeId;
  final String path;
  final String? parentPath;
  final List<ServerEntry> entries;

  ServerDirectoryListing({
    required this.nodeId,
    required this.path,
    required this.parentPath,
    required this.entries
  });

  factory ServerDirectoryListing.fromJson(Map<String, dynamic> json) {
    return ServerDirectoryListing(
      nodeId: (() {
        final value = json['nodeId']?.toString();
        if (value == null) {
          throw FormatException('ServerDirectoryListing.nodeId is required');
        }
        return value;
      })(),
      path: (() {
        final value = json['path']?.toString();
        if (value == null) {
          throw FormatException('ServerDirectoryListing.path is required');
        }
        return value;
      })(),
      parentPath: (() {
        if (!json.containsKey('parentPath')) {
          throw FormatException('ServerDirectoryListing.parentPath is required');
        }
        final _sdkworkRequiredValue = json['parentPath'];
        if (_sdkworkRequiredValue == null) {
          return null;
        }
        return (() {
        final value = _sdkworkRequiredValue?.toString();
        if (value == null) {
          throw FormatException('ServerDirectoryListing.parentPath is required');
        }
        return value;
      })();
      })(),
      entries: (() {
        final list = _sdkworkAsList(json['entries']);
        if (list == null) {
          throw FormatException('ServerDirectoryListing.entries is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : ServerEntry.fromJson(map);
      })())
            .whereType<ServerEntry>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'nodeId': nodeId,
      'path': path,
      'parentPath': parentPath,
      'entries': entries.map((item) => item.toJson()).toList(),
    };
  }
}

class ServerEntry {
  final String name;
  final String kind;
  final String path;
  final String? size;
  final String? projectType;
  final bool? isProjectRoot;

  ServerEntry({
    required this.name,
    required this.kind,
    required this.path,
    this.size,
    this.projectType,
    this.isProjectRoot
  });

  factory ServerEntry.fromJson(Map<String, dynamic> json) {
    return ServerEntry(
      name: (() {
        final value = json['name']?.toString();
        if (value == null) {
          throw FormatException('ServerEntry.name is required');
        }
        return value;
      })(),
      kind: (() {
        final value = json['kind']?.toString();
        if (value == null) {
          throw FormatException('ServerEntry.kind is required');
        }
        return value;
      })(),
      path: (() {
        final value = json['path']?.toString();
        if (value == null) {
          throw FormatException('ServerEntry.path is required');
        }
        return value;
      })(),
      size: json['size']?.toString(),
      projectType: json['projectType']?.toString(),
      isProjectRoot: json['isProjectRoot'] is bool ? json['isProjectRoot'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'kind': kind,
      'path': path,
      'size': size,
      'projectType': projectType,
      'isProjectRoot': isProjectRoot,
    };
  }
}

class ServerFileContent {
  final String nodeId;
  final String path;
  final String content;
  final String size;

  ServerFileContent({
    required this.nodeId,
    required this.path,
    required this.content,
    required this.size
  });

  factory ServerFileContent.fromJson(Map<String, dynamic> json) {
    return ServerFileContent(
      nodeId: (() {
        final value = json['nodeId']?.toString();
        if (value == null) {
          throw FormatException('ServerFileContent.nodeId is required');
        }
        return value;
      })(),
      path: (() {
        final value = json['path']?.toString();
        if (value == null) {
          throw FormatException('ServerFileContent.path is required');
        }
        return value;
      })(),
      content: (() {
        final value = json['content']?.toString();
        if (value == null) {
          throw FormatException('ServerFileContent.content is required');
        }
        return value;
      })(),
      size: (() {
        final value = json['size']?.toString();
        if (value == null) {
          throw FormatException('ServerFileContent.size is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'nodeId': nodeId,
      'path': path,
      'content': content,
      'size': size,
    };
  }
}

class ServerProjectOperations {
  final String nodeId;
  final String path;
  final String projectType;
  final List<ServerProjectOperation> operations;

  ServerProjectOperations({
    required this.nodeId,
    required this.path,
    required this.projectType,
    required this.operations
  });

  factory ServerProjectOperations.fromJson(Map<String, dynamic> json) {
    return ServerProjectOperations(
      nodeId: (() {
        final value = json['nodeId']?.toString();
        if (value == null) {
          throw FormatException('ServerProjectOperations.nodeId is required');
        }
        return value;
      })(),
      path: (() {
        final value = json['path']?.toString();
        if (value == null) {
          throw FormatException('ServerProjectOperations.path is required');
        }
        return value;
      })(),
      projectType: (() {
        final value = json['projectType']?.toString();
        if (value == null) {
          throw FormatException('ServerProjectOperations.projectType is required');
        }
        return value;
      })(),
      operations: (() {
        final list = _sdkworkAsList(json['operations']);
        if (list == null) {
          throw FormatException('ServerProjectOperations.operations is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : ServerProjectOperation.fromJson(map);
      })())
            .whereType<ServerProjectOperation>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'nodeId': nodeId,
      'path': path,
      'projectType': projectType,
      'operations': operations.map((item) => item.toJson()).toList(),
    };
  }
}

class ServerProjectOperation {
  final String id;
  final String kind;
  final String label;
  final String permission;
  final String? description;
  final bool? dangerous;

  ServerProjectOperation({
    required this.id,
    required this.kind,
    required this.label,
    required this.permission,
    this.description,
    this.dangerous
  });

  factory ServerProjectOperation.fromJson(Map<String, dynamic> json) {
    return ServerProjectOperation(
      id: (() {
        final value = json['id']?.toString();
        if (value == null) {
          throw FormatException('ServerProjectOperation.id is required');
        }
        return value;
      })(),
      kind: (() {
        final value = json['kind']?.toString();
        if (value == null) {
          throw FormatException('ServerProjectOperation.kind is required');
        }
        return value;
      })(),
      label: (() {
        final value = json['label']?.toString();
        if (value == null) {
          throw FormatException('ServerProjectOperation.label is required');
        }
        return value;
      })(),
      permission: (() {
        final value = json['permission']?.toString();
        if (value == null) {
          throw FormatException('ServerProjectOperation.permission is required');
        }
        return value;
      })(),
      description: json['description']?.toString(),
      dangerous: json['dangerous'] is bool ? json['dangerous'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'kind': kind,
      'label': label,
      'permission': permission,
      'description': description,
      'dangerous': dangerous,
    };
  }
}

class ServerRunOperationRequest {
  final String path;
  final String operationId;

  ServerRunOperationRequest({
    required this.path,
    required this.operationId
  });

  factory ServerRunOperationRequest.fromJson(Map<String, dynamic> json) {
    return ServerRunOperationRequest(
      path: (() {
        final value = json['path']?.toString();
        if (value == null) {
          throw FormatException('ServerRunOperationRequest.path is required');
        }
        return value;
      })(),
      operationId: (() {
        final value = json['operationId']?.toString();
        if (value == null) {
          throw FormatException('ServerRunOperationRequest.operationId is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'path': path,
      'operationId': operationId,
    };
  }
}

class ServerOperationResult {
  final String operationId;
  final int? exitCode;
  final String? stdout;
  final String? stderr;

  ServerOperationResult({
    required this.operationId,
    this.exitCode,
    this.stdout,
    this.stderr
  });

  factory ServerOperationResult.fromJson(Map<String, dynamic> json) {
    return ServerOperationResult(
      operationId: (() {
        final value = json['operationId']?.toString();
        if (value == null) {
          throw FormatException('ServerOperationResult.operationId is required');
        }
        return value;
      })(),
      exitCode: json['exitCode'] is int ? json['exitCode'] : null,
      stdout: json['stdout']?.toString(),
      stderr: json['stderr']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'operationId': operationId,
      'exitCode': exitCode,
      'stdout': stdout,
      'stderr': stderr,
    };
  }
}

class AgentHeartbeatRequest {
  final String? agentVersion;
  final bool? nginxEnabled;
  final String? activeConfigs;
  final String? lastSyncVersion;
  final List<AgentCertificateObservation>? certificateObservations;

  AgentHeartbeatRequest({
    this.agentVersion,
    this.nginxEnabled,
    this.activeConfigs,
    this.lastSyncVersion,
    this.certificateObservations
  });

  factory AgentHeartbeatRequest.fromJson(Map<String, dynamic> json) {
    return AgentHeartbeatRequest(
      agentVersion: json['agentVersion']?.toString(),
      nginxEnabled: json['nginxEnabled'] is bool ? json['nginxEnabled'] : null,
      activeConfigs: json['activeConfigs']?.toString(),
      lastSyncVersion: json['lastSyncVersion']?.toString(),
      certificateObservations: (() {
        final list = _sdkworkAsList(json['certificateObservations']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AgentCertificateObservation.fromJson(map);
      })())
            .whereType<AgentCertificateObservation>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'agentVersion': agentVersion,
      'nginxEnabled': nginxEnabled,
      'activeConfigs': activeConfigs,
      'lastSyncVersion': lastSyncVersion,
      'certificateObservations': certificateObservations?.map((item) => item.toJson()).toList(),
    };
  }
}

class AgentCertificateObservation {
  final String certificateId;
  final String fingerprint;
  final String syncVersion;
  final String state;
  final String observedAt;
  final String? failureCode;

  AgentCertificateObservation({
    required this.certificateId,
    required this.fingerprint,
    required this.syncVersion,
    required this.state,
    required this.observedAt,
    this.failureCode
  });

  factory AgentCertificateObservation.fromJson(Map<String, dynamic> json) {
    return AgentCertificateObservation(
      certificateId: (() {
        final value = json['certificateId']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateObservation.certificateId is required');
        }
        return value;
      })(),
      fingerprint: (() {
        final value = json['fingerprint']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateObservation.fingerprint is required');
        }
        return value;
      })(),
      syncVersion: (() {
        final value = json['syncVersion']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateObservation.syncVersion is required');
        }
        return value;
      })(),
      state: (() {
        final value = json['state']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateObservation.state is required');
        }
        return value;
      })(),
      observedAt: (() {
        final value = json['observedAt']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateObservation.observedAt is required');
        }
        return value;
      })(),
      failureCode: json['failureCode']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'certificateId': certificateId,
      'fingerprint': fingerprint,
      'syncVersion': syncVersion,
      'state': state,
      'observedAt': observedAt,
      'failureCode': failureCode,
    };
  }
}

class AgentHeartbeatResponse {
  final String serverId;
  final int status;
  final String acknowledgedAt;

  AgentHeartbeatResponse({
    required this.serverId,
    required this.status,
    required this.acknowledgedAt
  });

  factory AgentHeartbeatResponse.fromJson(Map<String, dynamic> json) {
    return AgentHeartbeatResponse(
      serverId: (() {
        final value = json['serverId']?.toString();
        if (value == null) {
          throw FormatException('AgentHeartbeatResponse.serverId is required');
        }
        return value;
      })(),
      status: (() {
        final value = json['status'];
        if (value is! int) {
          throw FormatException('AgentHeartbeatResponse.status is required');
        }
        return value;
      })(),
      acknowledgedAt: (() {
        final value = json['acknowledgedAt']?.toString();
        if (value == null) {
          throw FormatException('AgentHeartbeatResponse.acknowledgedAt is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'serverId': serverId,
      'status': status,
      'acknowledgedAt': acknowledgedAt,
    };
  }
}

class AgentSyncResponse {
  final String serverId;
  final String syncVersion;
  final bool unchanged;
  final List<AgentNginxConfigBundle> nginxConfigs;
  final List<AgentCertificateBundle> certificates;

  AgentSyncResponse({
    required this.serverId,
    required this.syncVersion,
    required this.unchanged,
    required this.nginxConfigs,
    required this.certificates
  });

  factory AgentSyncResponse.fromJson(Map<String, dynamic> json) {
    return AgentSyncResponse(
      serverId: (() {
        final value = json['serverId']?.toString();
        if (value == null) {
          throw FormatException('AgentSyncResponse.serverId is required');
        }
        return value;
      })(),
      syncVersion: (() {
        final value = json['syncVersion']?.toString();
        if (value == null) {
          throw FormatException('AgentSyncResponse.syncVersion is required');
        }
        return value;
      })(),
      unchanged: (() {
        final value = json['unchanged'];
        if (value is! bool) {
          throw FormatException('AgentSyncResponse.unchanged is required');
        }
        return value;
      })(),
      nginxConfigs: (() {
        final list = _sdkworkAsList(json['nginxConfigs']);
        if (list == null) {
          throw FormatException('AgentSyncResponse.nginxConfigs is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AgentNginxConfigBundle.fromJson(map);
      })())
            .whereType<AgentNginxConfigBundle>()
            .toList();
      })(),
      certificates: (() {
        final list = _sdkworkAsList(json['certificates']);
        if (list == null) {
          throw FormatException('AgentSyncResponse.certificates is required');
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AgentCertificateBundle.fromJson(map);
      })())
            .whereType<AgentCertificateBundle>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'serverId': serverId,
      'syncVersion': syncVersion,
      'unchanged': unchanged,
      'nginxConfigs': nginxConfigs.map((item) => item.toJson()).toList(),
      'certificates': certificates.map((item) => item.toJson()).toList(),
    };
  }
}

class AgentNginxConfigBundle {
  final String configId;
  final String domain;
  final String configContent;
  final String fingerprint;
  final String version;

  AgentNginxConfigBundle({
    required this.configId,
    required this.domain,
    required this.configContent,
    required this.fingerprint,
    required this.version
  });

  factory AgentNginxConfigBundle.fromJson(Map<String, dynamic> json) {
    return AgentNginxConfigBundle(
      configId: (() {
        final value = json['configId']?.toString();
        if (value == null) {
          throw FormatException('AgentNginxConfigBundle.configId is required');
        }
        return value;
      })(),
      domain: (() {
        final value = json['domain']?.toString();
        if (value == null) {
          throw FormatException('AgentNginxConfigBundle.domain is required');
        }
        return value;
      })(),
      configContent: (() {
        final value = json['configContent']?.toString();
        if (value == null) {
          throw FormatException('AgentNginxConfigBundle.configContent is required');
        }
        return value;
      })(),
      fingerprint: (() {
        final value = json['fingerprint']?.toString();
        if (value == null) {
          throw FormatException('AgentNginxConfigBundle.fingerprint is required');
        }
        return value;
      })(),
      version: (() {
        final value = json['version']?.toString();
        if (value == null) {
          throw FormatException('AgentNginxConfigBundle.version is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'configId': configId,
      'domain': domain,
      'configContent': configContent,
      'fingerprint': fingerprint,
      'version': version,
    };
  }
}

class AgentCertificateBundle {
  final String certificateId;
  final String certName;
  final String fingerprint;
  final List<String> hostnames;
  final String fullchainPem;
  final String privkeyPem;

  AgentCertificateBundle({
    required this.certificateId,
    required this.certName,
    required this.fingerprint,
    required this.hostnames,
    required this.fullchainPem,
    required this.privkeyPem
  });

  factory AgentCertificateBundle.fromJson(Map<String, dynamic> json) {
    return AgentCertificateBundle(
      certificateId: (() {
        final value = json['certificateId']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateBundle.certificateId is required');
        }
        return value;
      })(),
      certName: (() {
        final value = json['certName']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateBundle.certName is required');
        }
        return value;
      })(),
      fingerprint: (() {
        final value = json['fingerprint']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateBundle.fingerprint is required');
        }
        return value;
      })(),
      hostnames: (() {
        final list = _sdkworkAsList(json['hostnames']);
        if (list == null) {
          throw FormatException('AgentCertificateBundle.hostnames is required');
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      fullchainPem: (() {
        final value = json['fullchainPem']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateBundle.fullchainPem is required');
        }
        return value;
      })(),
      privkeyPem: (() {
        final value = json['privkeyPem']?.toString();
        if (value == null) {
          throw FormatException('AgentCertificateBundle.privkeyPem is required');
        }
        return value;
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'certificateId': certificateId,
      'certName': certName,
      'fingerprint': fingerprint,
      'hostnames': hostnames.map((item) => item).toList(),
      'fullchainPem': fullchainPem,
      'privkeyPem': privkeyPem,
    };
  }
}

class ServerPage {
  final List<ServerResponse>? items;
  final String? total;

  ServerPage({
    this.items,
    this.total
  });

  factory ServerPage.fromJson(Map<String, dynamic> json) {
    return ServerPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : ServerResponse.fromJson(map);
      })())
            .whereType<ServerResponse>()
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

class AuditLogResponse {
  final String? id;
  final String? operatorId;
  final String? operatorType;
  final String? action;
  final String? targetType;
  final String? targetId;
  final String? targetUuid;
  final String? ipAddress;
  final Map<String, dynamic>? changes;
  final String? createdAt;

  AuditLogResponse({
    this.id,
    this.operatorId,
    this.operatorType,
    this.action,
    this.targetType,
    this.targetId,
    this.targetUuid,
    this.ipAddress,
    this.changes,
    this.createdAt
  });

  factory AuditLogResponse.fromJson(Map<String, dynamic> json) {
    return AuditLogResponse(
      id: json['id']?.toString(),
      operatorId: json['operatorId']?.toString(),
      operatorType: json['operatorType']?.toString(),
      action: json['action']?.toString(),
      targetType: json['targetType']?.toString(),
      targetId: json['targetId']?.toString(),
      targetUuid: json['targetUuid']?.toString(),
      ipAddress: json['ipAddress']?.toString(),
      changes: _sdkworkAsMap(json['changes']),
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'operatorId': operatorId,
      'operatorType': operatorType,
      'action': action,
      'targetType': targetType,
      'targetId': targetId,
      'targetUuid': targetUuid,
      'ipAddress': ipAddress,
      'changes': changes,
      'createdAt': createdAt,
    };
  }
}

class AuditLogPage {
  final List<AuditLogResponse>? items;
  final String? total;

  AuditLogPage({
    this.items,
    this.total
  });

  factory AuditLogPage.fromJson(Map<String, dynamic> json) {
    return AuditLogPage(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : AuditLogResponse.fromJson(map);
      })())
            .whereType<AuditLogResponse>()
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

class RootDomainsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  RootDomainsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory RootDomainsListResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('RootDomainsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('RootDomainsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('RootDomainsListResponse.traceId is required');
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

class RootDomainsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  RootDomainsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory RootDomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return RootDomainsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('RootDomainsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('RootDomainsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('RootDomainsCreateResponse201.traceId is required');
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

class RootDomainsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  RootDomainsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory RootDomainsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('RootDomainsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('RootDomainsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('RootDomainsRetrieveResponse.traceId is required');
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

class RootDomainsSubdomainsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  RootDomainsSubdomainsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory RootDomainsSubdomainsListResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainsSubdomainsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('RootDomainsSubdomainsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('RootDomainsSubdomainsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('RootDomainsSubdomainsListResponse.traceId is required');
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

class RootDomainsSubdomainsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  RootDomainsSubdomainsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory RootDomainsSubdomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return RootDomainsSubdomainsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('RootDomainsSubdomainsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('RootDomainsSubdomainsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('RootDomainsSubdomainsCreateResponse201.traceId is required');
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

class DomainsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  DomainsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return DomainsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainsCreateResponse201.traceId is required');
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

class DomainsVerifyResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainsVerifyResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainsVerifyResponse.fromJson(Map<String, dynamic> json) {
    return DomainsVerifyResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainsVerifyResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainsVerifyResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainsVerifyResponse.traceId is required');
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

class DomainsApplicationBindingUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  DomainsApplicationBindingUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory DomainsApplicationBindingUpdateResponse.fromJson(Map<String, dynamic> json) {
    return DomainsApplicationBindingUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('DomainsApplicationBindingUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('DomainsApplicationBindingUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('DomainsApplicationBindingUpdateResponse.traceId is required');
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

class CertificatesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesListResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesListResponse.traceId is required');
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

class CertificatesIssueResponse202 {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesIssueResponse202({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesIssueResponse202.fromJson(Map<String, dynamic> json) {
    return CertificatesIssueResponse202(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesIssueResponse202.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesIssueResponse202.traceId is required');
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

class CertificatesOperationsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesOperationsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesOperationsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesOperationsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesOperationsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesOperationsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesOperationsRetrieveResponse.traceId is required');
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

class CertificatesUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesUpdateResponse.traceId is required');
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

class CertificatesRenewResponse202 {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesRenewResponse202({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesRenewResponse202.fromJson(Map<String, dynamic> json) {
    return CertificatesRenewResponse202(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesRenewResponse202.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesRenewResponse202.traceId is required');
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

class CertificatesRevokeResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesRevokeResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesRevokeResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesRevokeResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesRevokeResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesRevokeResponse.traceId is required');
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

class CertificatesDistributionListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  CertificatesDistributionListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory CertificatesDistributionListResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesDistributionListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('CertificatesDistributionListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('CertificatesDistributionListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('CertificatesDistributionListResponse.traceId is required');
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

class ConfigsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ConfigsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ConfigsListResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ConfigsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ConfigsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ConfigsListResponse.traceId is required');
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

class ConfigsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ConfigsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ConfigsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ConfigsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ConfigsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ConfigsCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ConfigsCreateResponse201.traceId is required');
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

class ConfigsRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ConfigsRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ConfigsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ConfigsRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ConfigsRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ConfigsRetrieveResponse.traceId is required');
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

class ConfigsUpdateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ConfigsUpdateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ConfigsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsUpdateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ConfigsUpdateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ConfigsUpdateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ConfigsUpdateResponse.traceId is required');
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

class ConfigsValidateResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ConfigsValidateResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ConfigsValidateResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsValidateResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ConfigsValidateResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ConfigsValidateResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ConfigsValidateResponse.traceId is required');
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

class ConfigsDeployResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ConfigsDeployResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ConfigsDeployResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsDeployResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ConfigsDeployResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ConfigsDeployResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ConfigsDeployResponse.traceId is required');
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

class ReloadResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ReloadResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ReloadResponse.fromJson(Map<String, dynamic> json) {
    return ReloadResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ReloadResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ReloadResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ReloadResponse.traceId is required');
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

class StatusRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  StatusRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory StatusRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return StatusRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('StatusRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('StatusRetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('StatusRetrieveResponse.traceId is required');
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

class ServersListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ServersListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ServersListResponse.fromJson(Map<String, dynamic> json) {
    return ServersListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ServersListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ServersListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ServersListResponse.traceId is required');
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

class ServersCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ServersCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ServersCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ServersCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ServersCreateResponse201.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ServersCreateResponse201.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ServersCreateResponse201.traceId is required');
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

class ServerFilesNodesListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ServerFilesNodesListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ServerFilesNodesListResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodesListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ServerFilesNodesListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('ServerFilesNodesListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNodesListResponse.traceId is required');
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

class ServerFilesNodeDirectoryListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ServerFilesNodeDirectoryListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ServerFilesNodeDirectoryListResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeDirectoryListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ServerFilesNodeDirectoryListResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNodeDirectoryListResponse.traceId is required');
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

class ServerFilesNodeRetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ServerFilesNodeRetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ServerFilesNodeRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeRetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ServerFilesNodeRetrieveResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNodeRetrieveResponse.traceId is required');
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

class ServerFilesNodeOperationsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  ServerFilesNodeOperationsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ServerFilesNodeOperationsListResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeOperationsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ServerFilesNodeOperationsListResponse.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNodeOperationsListResponse.traceId is required');
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

class ServerFilesNodeOperationsCreateResponse201 {
  final int code;
  final dynamic data;
  final String traceId;

  ServerFilesNodeOperationsCreateResponse201({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory ServerFilesNodeOperationsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeOperationsCreateResponse201(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('ServerFilesNodeOperationsCreateResponse201.code is required');
        }
        return value;
      })(),
      data: json['data'],
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('ServerFilesNodeOperationsCreateResponse201.traceId is required');
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

class HeartbeatResponse {
  final int code;
  final dynamic data;
  final String traceId;

  HeartbeatResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory HeartbeatResponse.fromJson(Map<String, dynamic> json) {
    return HeartbeatResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('HeartbeatResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('HeartbeatResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('HeartbeatResponse.traceId is required');
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

class RetrieveResponse {
  final int code;
  final dynamic data;
  final String traceId;

  RetrieveResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory RetrieveResponse.fromJson(Map<String, dynamic> json) {
    return RetrieveResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('RetrieveResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('RetrieveResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('RetrieveResponse.traceId is required');
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

class AuditLogsListResponse {
  final int code;
  final dynamic data;
  final String traceId;

  AuditLogsListResponse({
    required this.code,
    required this.data,
    required this.traceId
  });

  factory AuditLogsListResponse.fromJson(Map<String, dynamic> json) {
    return AuditLogsListResponse(
      code: (() {
        final value = json['code'];
        if (value is! int) {
          throw FormatException('AuditLogsListResponse.code is required');
        }
        return value;
      })(),
      data: (() {
        final map = _sdkworkAsMap(json['data']);
        if (map == null) {
          throw FormatException('AuditLogsListResponse.data is required');
        }
        return map;
      })(),
      traceId: (() {
        final value = json['traceId']?.toString();
        if (value == null) {
          throw FormatException('AuditLogsListResponse.traceId is required');
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
