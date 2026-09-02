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
  final String? type;
  final String? title;
  final int? status;
  final String? detail;
  final String? instance;
  final int? code;
  final String? traceId;
  final List<FieldError>? errors;

  ProblemDetail({
    this.type,
    this.title,
    this.status,
    this.detail,
    this.instance,
    this.code,
    this.traceId,
    this.errors
  });

  factory ProblemDetail.fromJson(Map<String, dynamic> json) {
    return ProblemDetail(
      type: json['type']?.toString(),
      title: json['title']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      detail: json['detail']?.toString(),
      instance: json['instance']?.toString(),
      code: json['code'] is int ? json['code'] : null,
      traceId: json['traceId']?.toString(),
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
  final int? configType;
  final String? configName;
  final String? configContent;
  final String? siteId;

  CreateNginxConfigRequest({
    this.configType,
    this.configName,
    this.configContent,
    this.siteId
  });

  factory CreateNginxConfigRequest.fromJson(Map<String, dynamic> json) {
    return CreateNginxConfigRequest(
      configType: json['configType'] is int ? json['configType'] : null,
      configName: json['configName']?.toString(),
      configContent: json['configContent']?.toString(),
      siteId: json['siteId']?.toString()
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
  final String? algorithm;
  final String? value;

  MediaChecksum({
    this.algorithm,
    this.value
  });

  factory MediaChecksum.fromJson(Map<String, dynamic> json) {
    return MediaChecksum(
      algorithm: json['algorithm']?.toString(),
      value: json['value']?.toString()
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
  final String? kind;
  final String? source;
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
    this.kind,
    this.source,
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
      kind: json['kind']?.toString(),
      source: json['source']?.toString(),
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
  final String? targetKey;
  final String? platform;
  final String? techStack;
  final List<String>? architectures;
  final String? bundleId;
  final String? packageName;
  final String? appId;
  final String? bundleName;
  final List<String>? allowedChannels;

  CreatePlatformTargetRequest({
    this.targetKey,
    this.platform,
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
  final String? name;
  final String? slug;
  final String? description;
  final String? appKind;
  final Map<String, dynamic>? runtimeConfig;
  final ApplicationStoreListing? storeListing;

  CreateApplicationRequest({
    this.name,
    this.slug,
    this.description,
    this.appKind,
    this.runtimeConfig,
    this.storeListing
  });

  factory CreateApplicationRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationRequest(
      name: json['name']?.toString(),
      slug: json['slug']?.toString(),
      description: json['description']?.toString(),
      appKind: json['appKind']?.toString(),
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
  final String? hostname;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateApplicationDomainRequest({
    this.hostname,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateApplicationDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationDomainRequest(
      hostname: json['hostname']?.toString(),
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
  final String? hostname;
  final String? applicationId;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateManagedDomainRequest({
    this.hostname,
    this.applicationId,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateManagedDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateManagedDomainRequest(
      hostname: json['hostname']?.toString(),
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
  final String? hostname;

  CreateRootDomainRequest({
    this.hostname
  });

  factory CreateRootDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateRootDomainRequest(
      hostname: json['hostname']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'hostname': hostname,
    };
  }
}

class CreateRootDomainHostnameRequest {
  final String? recordName;
  final String? applicationId;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateRootDomainHostnameRequest({
    this.recordName,
    this.applicationId,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateRootDomainHostnameRequest.fromJson(Map<String, dynamic> json) {
    return CreateRootDomainHostnameRequest(
      recordName: json['recordName']?.toString(),
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
  final String? id;
  final String? hostname;
  final int? status;
  final String? subdomainCount;
  final String? boundSubdomainCount;
  final String? verifiedSubdomainCount;
  final String? httpsSubdomainCount;
  final String? activeDeploymentCount;
  final String? createdAt;
  final String? updatedAt;

  RootDomainResponse({
    this.id,
    this.hostname,
    this.status,
    this.subdomainCount,
    this.boundSubdomainCount,
    this.verifiedSubdomainCount,
    this.httpsSubdomainCount,
    this.activeDeploymentCount,
    this.createdAt,
    this.updatedAt
  });

  factory RootDomainResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainResponse(
      id: json['id']?.toString(),
      hostname: json['hostname']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      subdomainCount: json['subdomainCount']?.toString(),
      boundSubdomainCount: json['boundSubdomainCount']?.toString(),
      verifiedSubdomainCount: json['verifiedSubdomainCount']?.toString(),
      httpsSubdomainCount: json['httpsSubdomainCount']?.toString(),
      activeDeploymentCount: json['activeDeploymentCount']?.toString(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
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
  final String? id;
  final int? status;
  final String? environment;
  final String? versionTag;
  final String? completedAt;
  final String? createdAt;

  DomainDeploymentResponse({
    this.id,
    this.status,
    this.environment,
    this.versionTag,
    this.completedAt,
    this.createdAt
  });

  factory DomainDeploymentResponse.fromJson(Map<String, dynamic> json) {
    return DomainDeploymentResponse(
      id: json['id']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      environment: json['environment']?.toString(),
      versionTag: json['versionTag']?.toString(),
      completedAt: json['completedAt']?.toString(),
      createdAt: json['createdAt']?.toString()
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
  final String? applicationId;
  final bool? isPrimary;

  UpdateDomainApplicationBindingRequest({
    this.applicationId,
    this.isPrimary
  });

  factory UpdateDomainApplicationBindingRequest.fromJson(Map<String, dynamic> json) {
    return UpdateDomainApplicationBindingRequest(
      applicationId: json['applicationId']?.toString(),
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
  final String? id;
  final String? hostname;
  final String? rootDomainId;
  final String? recordName;
  final String? applicationId;
  final String? applicationName;
  final String? certificateCount;
  final bool? isPrimary;
  final bool? isVerified;
  final bool? sslEnabled;
  final String? sslProvider;
  final int? status;
  final DomainDeploymentResponse? latestDeployment;
  final String? createdAt;
  final String? updatedAt;

  ApplicationDomainResponse({
    this.id,
    this.hostname,
    this.rootDomainId,
    this.recordName,
    this.applicationId,
    this.applicationName,
    this.certificateCount,
    this.isPrimary,
    this.isVerified,
    this.sslEnabled,
    this.sslProvider,
    this.status,
    this.latestDeployment,
    this.createdAt,
    this.updatedAt
  });

  factory ApplicationDomainResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationDomainResponse(
      id: json['id']?.toString(),
      hostname: json['hostname']?.toString(),
      rootDomainId: json['rootDomainId']?.toString(),
      recordName: json['recordName']?.toString(),
      applicationId: json['applicationId']?.toString(),
      applicationName: json['applicationName']?.toString(),
      certificateCount: json['certificateCount']?.toString(),
      isPrimary: json['isPrimary'] is bool ? json['isPrimary'] : null,
      isVerified: json['isVerified'] is bool ? json['isVerified'] : null,
      sslEnabled: json['sslEnabled'] is bool ? json['sslEnabled'] : null,
      sslProvider: json['sslProvider']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      latestDeployment: (() {
        final map = _sdkworkAsMap(json['latestDeployment']);
        return map == null ? null : DomainDeploymentResponse.fromJson(map);
      })(),
      createdAt: json['createdAt']?.toString(),
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
  final bool? verified;
  final String? status;
  final String? method;
  final String? recordName;
  final String? recordValue;
  final int? attemptCount;
  final String? expiresAt;
  final String? nextAttemptAt;
  final String? checkedAt;
  final String? failureCode;

  DomainVerifyResponse({
    this.verified,
    this.status,
    this.method,
    this.recordName,
    this.recordValue,
    this.attemptCount,
    this.expiresAt,
    this.nextAttemptAt,
    this.checkedAt,
    this.failureCode
  });

  factory DomainVerifyResponse.fromJson(Map<String, dynamic> json) {
    return DomainVerifyResponse(
      verified: json['verified'] is bool ? json['verified'] : null,
      status: json['status']?.toString(),
      method: json['method']?.toString(),
      recordName: json['recordName']?.toString(),
      recordValue: json['recordValue']?.toString(),
      attemptCount: json['attemptCount'] is int ? json['attemptCount'] : null,
      expiresAt: json['expiresAt']?.toString(),
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
  final String? appConfigPath;
  final String? deploymentConfigPath;
  final bool? appConfigDetected;
  final bool? deploymentConfigDetected;

  ApplicationSourceVersionConfigSnapshot({
    this.appConfigPath,
    this.deploymentConfigPath,
    this.appConfigDetected,
    this.deploymentConfigDetected
  });

  factory ApplicationSourceVersionConfigSnapshot.fromJson(Map<String, dynamic> json) {
    return ApplicationSourceVersionConfigSnapshot(
      appConfigPath: json['appConfigPath']?.toString(),
      deploymentConfigPath: json['deploymentConfigPath']?.toString(),
      appConfigDetected: json['appConfigDetected'] is bool ? json['appConfigDetected'] : null,
      deploymentConfigDetected: json['deploymentConfigDetected'] is bool ? json['deploymentConfigDetected'] : null
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
  final String? versionTag;
  final String? sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final ApplicationSourceVersionConfigSnapshot? configSnapshot;

  CreateApplicationSourceVersionRequest({
    this.versionTag,
    this.sourceType,
    this.sourceRef,
    this.commitHash,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash,
    this.configSnapshot
  });

  factory CreateApplicationSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationSourceVersionRequest(
      versionTag: json['versionTag']?.toString(),
      sourceType: json['sourceType']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
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
  final String? versionTag;
  final String? repositoryUrl;
  final String? gitRef;

  ImportApplicationGitSourceVersionRequest({
    this.versionTag,
    this.repositoryUrl,
    this.gitRef
  });

  factory ImportApplicationGitSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return ImportApplicationGitSourceVersionRequest(
      versionTag: json['versionTag']?.toString(),
      repositoryUrl: json['repositoryUrl']?.toString(),
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
  final String? id;
  final String? siteId;
  final String? versionTag;
  final String? sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final ApplicationSourceVersionConfigSnapshot? configSnapshot;
  final int? status;
  final bool? retained;
  final String? createdAt;

  ApplicationSourceVersionResponse({
    this.id,
    this.siteId,
    this.versionTag,
    this.sourceType,
    this.sourceRef,
    this.commitHash,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash,
    this.configSnapshot,
    this.status,
    this.retained,
    this.createdAt
  });

  factory ApplicationSourceVersionResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationSourceVersionResponse(
      id: json['id']?.toString(),
      siteId: json['siteId']?.toString(),
      versionTag: json['versionTag']?.toString(),
      sourceType: json['sourceType']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
      configSnapshot: (() {
        final map = _sdkworkAsMap(json['configSnapshot']);
        return map == null ? null : ApplicationSourceVersionConfigSnapshot.fromJson(map);
      })(),
      status: json['status'] is int ? json['status'] : null,
      retained: json['retained'] is bool ? json['retained'] : null,
      createdAt: json['createdAt']?.toString()
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
      'configSnapshot': configSnapshot?.toJson(),
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
  final String? id;
  final String? siteId;
  final String? sourceVersionId;
  final int? status;
  final int? deployType;
  final String? environment;
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
  final String? createdAt;

  ApplicationDeploymentResponse({
    this.id,
    this.siteId,
    this.sourceVersionId,
    this.status,
    this.deployType,
    this.environment,
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
    this.createdAt
  });

  factory ApplicationDeploymentResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationDeploymentResponse(
      id: json['id']?.toString(),
      siteId: json['siteId']?.toString(),
      sourceVersionId: json['sourceVersionId']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      deployType: json['deployType'] is int ? json['deployType'] : null,
      environment: json['environment']?.toString(),
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
      createdAt: json['createdAt']?.toString()
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
  final List<String>? domainIds;
  final int? certType;
  final String? keyAlgorithm;
  final bool? autoRenew;

  IssueCertificateRequest({
    this.domainIds,
    this.certType,
    this.keyAlgorithm,
    this.autoRenew
  });

  factory IssueCertificateRequest.fromJson(Map<String, dynamic> json) {
    return IssueCertificateRequest(
      domainIds: (() {
        final list = _sdkworkAsList(json['domainIds']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      certType: json['certType'] is int ? json['certType'] : null,
      keyAlgorithm: json['keyAlgorithm']?.toString(),
      autoRenew: json['autoRenew'] is bool ? json['autoRenew'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'domainIds': domainIds?.map((item) => item).toList(),
      'certType': certType,
      'keyAlgorithm': keyAlgorithm,
      'autoRenew': autoRenew,
    };
  }
}

class CertificateIdentifierResponse {
  final String? domainId;
  final String? hostname;
  final String? identifierType;
  final int? position;

  CertificateIdentifierResponse({
    this.domainId,
    this.hostname,
    this.identifierType,
    this.position
  });

  factory CertificateIdentifierResponse.fromJson(Map<String, dynamic> json) {
    return CertificateIdentifierResponse(
      domainId: json['domainId']?.toString(),
      hostname: json['hostname']?.toString(),
      identifierType: json['identifierType']?.toString(),
      position: json['position'] is int ? json['position'] : null
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
  final bool? autoRenew;

  UpdateCertificateRequest({
    this.autoRenew
  });

  factory UpdateCertificateRequest.fromJson(Map<String, dynamic> json) {
    return UpdateCertificateRequest(
      autoRenew: json['autoRenew'] is bool ? json['autoRenew'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'autoRenew': autoRenew,
    };
  }
}

class CertificateResponse {
  final String? id;
  final String? certName;
  final List<CertificateIdentifierResponse>? identifiers;
  final int? certType;
  final String? issuer;
  final String? fingerprint;
  final String? keyAlgorithm;
  final String? notBefore;
  final String? notAfter;
  final bool? autoRenew;
  final String? renewalStatus;
  final String? status;
  final String? createdAt;

  CertificateResponse({
    this.id,
    this.certName,
    this.identifiers,
    this.certType,
    this.issuer,
    this.fingerprint,
    this.keyAlgorithm,
    this.notBefore,
    this.notAfter,
    this.autoRenew,
    this.renewalStatus,
    this.status,
    this.createdAt
  });

  factory CertificateResponse.fromJson(Map<String, dynamic> json) {
    return CertificateResponse(
      id: json['id']?.toString(),
      certName: json['certName']?.toString(),
      identifiers: (() {
        final list = _sdkworkAsList(json['identifiers']);
        if (list == null) {
          return null;
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
      keyAlgorithm: json['keyAlgorithm']?.toString(),
      notBefore: json['notBefore']?.toString(),
      notAfter: json['notAfter']?.toString(),
      autoRenew: json['autoRenew'] is bool ? json['autoRenew'] : null,
      renewalStatus: json['renewalStatus']?.toString(),
      status: json['status']?.toString(),
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'certName': certName,
      'identifiers': identifiers?.map((item) => item.toJson()).toList(),
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
  final String? reason;

  RevokeCertificateRequest({
    this.reason
  });

  factory RevokeCertificateRequest.fromJson(Map<String, dynamic> json) {
    return RevokeCertificateRequest(
      reason: json['reason']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'reason': reason,
    };
  }
}

class CertificateOperationResponse {
  final String? id;
  final String? certificateId;
  final String? operationType;
  final String? status;
  final int? attemptCount;
  final int? maxAttempts;
  final String? nextAttemptAt;
  final String? failureCode;
  final String? createdAt;
  final String? updatedAt;
  final String? completedAt;

  CertificateOperationResponse({
    this.id,
    this.certificateId,
    this.operationType,
    this.status,
    this.attemptCount,
    this.maxAttempts,
    this.nextAttemptAt,
    this.failureCode,
    this.createdAt,
    this.updatedAt,
    this.completedAt
  });

  factory CertificateOperationResponse.fromJson(Map<String, dynamic> json) {
    return CertificateOperationResponse(
      id: json['id']?.toString(),
      certificateId: json['certificateId']?.toString(),
      operationType: json['operationType']?.toString(),
      status: json['status']?.toString(),
      attemptCount: json['attemptCount'] is int ? json['attemptCount'] : null,
      maxAttempts: json['maxAttempts'] is int ? json['maxAttempts'] : null,
      nextAttemptAt: json['nextAttemptAt']?.toString(),
      failureCode: json['failureCode']?.toString(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString(),
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
  final String? certificateId;
  final String? certificateVersionId;
  final int? priority;
  final bool? isDefault;

  CreateListenerCertificateBindingRequest({
    this.certificateId,
    this.certificateVersionId,
    this.priority,
    this.isDefault
  });

  factory CreateListenerCertificateBindingRequest.fromJson(Map<String, dynamic> json) {
    return CreateListenerCertificateBindingRequest(
      certificateId: json['certificateId']?.toString(),
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
  final String? id;
  final String? siteId;
  final String? domainId;
  final String? certificateId;
  final String? desiredCertificateVersionId;
  final String? currentCertificateVersionId;
  final ListenerCertificateSummaryResponse? desiredCertificate;
  final ListenerCertificateSummaryResponse? currentCertificate;
  final String? keyAlgorithm;
  final int? priority;
  final bool? isDefault;
  final String? status;
  final String? activatedAt;
  final String? createdAt;
  final String? updatedAt;

  ListenerCertificateBindingResponse({
    this.id,
    this.siteId,
    this.domainId,
    this.certificateId,
    this.desiredCertificateVersionId,
    this.currentCertificateVersionId,
    this.desiredCertificate,
    this.currentCertificate,
    this.keyAlgorithm,
    this.priority,
    this.isDefault,
    this.status,
    this.activatedAt,
    this.createdAt,
    this.updatedAt
  });

  factory ListenerCertificateBindingResponse.fromJson(Map<String, dynamic> json) {
    return ListenerCertificateBindingResponse(
      id: json['id']?.toString(),
      siteId: json['siteId']?.toString(),
      domainId: json['domainId']?.toString(),
      certificateId: json['certificateId']?.toString(),
      desiredCertificateVersionId: json['desiredCertificateVersionId']?.toString(),
      currentCertificateVersionId: json['currentCertificateVersionId']?.toString(),
      desiredCertificate: (() {
        final map = _sdkworkAsMap(json['desiredCertificate']);
        return map == null ? null : ListenerCertificateSummaryResponse.fromJson(map);
      })(),
      currentCertificate: (() {
        final map = _sdkworkAsMap(json['currentCertificate']);
        return map == null ? null : ListenerCertificateSummaryResponse.fromJson(map);
      })(),
      keyAlgorithm: json['keyAlgorithm']?.toString(),
      priority: json['priority'] is int ? json['priority'] : null,
      isDefault: json['isDefault'] is bool ? json['isDefault'] : null,
      status: json['status']?.toString(),
      activatedAt: json['activatedAt']?.toString(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
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
      'desiredCertificate': desiredCertificate?.toJson(),
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
  final String? certName;
  final List<CertificateIdentifierResponse>? identifiers;
  final String? issuer;
  final String? fingerprint;
  final String? notAfter;
  final String? status;

  ListenerCertificateSummaryResponse({
    this.certName,
    this.identifiers,
    this.issuer,
    this.fingerprint,
    this.notAfter,
    this.status
  });

  factory ListenerCertificateSummaryResponse.fromJson(Map<String, dynamic> json) {
    return ListenerCertificateSummaryResponse(
      certName: json['certName']?.toString(),
      identifiers: (() {
        final list = _sdkworkAsList(json['identifiers']);
        if (list == null) {
          return null;
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
      status: json['status']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'certName': certName,
      'identifiers': identifiers?.map((item) => item.toJson()).toList(),
      'issuer': issuer,
      'fingerprint': fingerprint,
      'notAfter': notAfter,
      'status': status,
    };
  }
}

class CertificateDistributionResponse {
  final String? serverId;
  final String? serverName;
  final String? host;
  final String? desiredSyncVersion;
  final String? appliedSyncVersion;
  final String? status;
  final String? lastHeartbeatAt;

  CertificateDistributionResponse({
    this.serverId,
    this.serverName,
    this.host,
    this.desiredSyncVersion,
    this.appliedSyncVersion,
    this.status,
    this.lastHeartbeatAt
  });

  factory CertificateDistributionResponse.fromJson(Map<String, dynamic> json) {
    return CertificateDistributionResponse(
      serverId: json['serverId']?.toString(),
      serverName: json['serverName']?.toString(),
      host: json['host']?.toString(),
      desiredSyncVersion: json['desiredSyncVersion']?.toString(),
      appliedSyncVersion: json['appliedSyncVersion']?.toString(),
      status: json['status']?.toString(),
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
  final String? name;
  final String? host;
  final String? tenantScopeHash;
  final int? sshPort;

  CreateServerRequest({
    this.name,
    this.host,
    this.tenantScopeHash,
    this.sshPort
  });

  factory CreateServerRequest.fromJson(Map<String, dynamic> json) {
    return CreateServerRequest(
      name: json['name']?.toString(),
      host: json['host']?.toString(),
      tenantScopeHash: json['tenantScopeHash']?.toString(),
      sshPort: json['sshPort'] is int ? json['sshPort'] : null
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
  final String? id;
  final String? name;
  final String? host;
  final String? tenantScopeHash;
  final int? sshPort;
  final int? status;
  final String? lastHeartbeatAt;
  final String? createdAt;

  ServerResponse({
    this.id,
    this.name,
    this.host,
    this.tenantScopeHash,
    this.sshPort,
    this.status,
    this.lastHeartbeatAt,
    this.createdAt
  });

  factory ServerResponse.fromJson(Map<String, dynamic> json) {
    return ServerResponse(
      id: json['id']?.toString(),
      name: json['name']?.toString(),
      host: json['host']?.toString(),
      tenantScopeHash: json['tenantScopeHash']?.toString(),
      sshPort: json['sshPort'] is int ? json['sshPort'] : null,
      status: json['status'] is int ? json['status'] : null,
      lastHeartbeatAt: json['lastHeartbeatAt']?.toString(),
      createdAt: json['createdAt']?.toString()
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
  final String? id;
  final String? name;
  final String? host;
  final String? tenantScopeHash;
  final int? sshPort;
  final int? status;
  final String? lastHeartbeatAt;
  final String? createdAt;
  final String? agentToken;

  CreateServerResponse({
    this.id,
    this.name,
    this.host,
    this.tenantScopeHash,
    this.sshPort,
    this.status,
    this.lastHeartbeatAt,
    this.createdAt,
    this.agentToken
  });

  factory CreateServerResponse.fromJson(Map<String, dynamic> json) {
    return CreateServerResponse(
      id: json['id']?.toString(),
      name: json['name']?.toString(),
      host: json['host']?.toString(),
      tenantScopeHash: json['tenantScopeHash']?.toString(),
      sshPort: json['sshPort'] is int ? json['sshPort'] : null,
      status: json['status'] is int ? json['status'] : null,
      lastHeartbeatAt: json['lastHeartbeatAt']?.toString(),
      createdAt: json['createdAt']?.toString(),
      agentToken: json['agentToken']?.toString()
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
  final String? id;
  final String? name;
  final String? host;
  final int? sshPort;
  final String? status;
  final String? filesystemRoot;
  final String? region;

  ServerFilesNode({
    this.id,
    this.name,
    this.host,
    this.sshPort,
    this.status,
    this.filesystemRoot,
    this.region
  });

  factory ServerFilesNode.fromJson(Map<String, dynamic> json) {
    return ServerFilesNode(
      id: json['id']?.toString(),
      name: json['name']?.toString(),
      host: json['host']?.toString(),
      sshPort: json['sshPort'] is int ? json['sshPort'] : null,
      status: json['status']?.toString(),
      filesystemRoot: json['filesystemRoot']?.toString(),
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
  final String? nodeId;
  final String? path;
  final String? parentPath;
  final List<ServerEntry>? entries;

  ServerDirectoryListing({
    this.nodeId,
    this.path,
    this.parentPath,
    this.entries
  });

  factory ServerDirectoryListing.fromJson(Map<String, dynamic> json) {
    return ServerDirectoryListing(
      nodeId: json['nodeId']?.toString(),
      path: json['path']?.toString(),
      parentPath: json['parentPath']?.toString(),
      entries: (() {
        final list = _sdkworkAsList(json['entries']);
        if (list == null) {
          return null;
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
      'entries': entries?.map((item) => item.toJson()).toList(),
    };
  }
}

class ServerEntry {
  final String? name;
  final String? kind;
  final String? path;
  final String? size;
  final String? projectType;
  final bool? isProjectRoot;

  ServerEntry({
    this.name,
    this.kind,
    this.path,
    this.size,
    this.projectType,
    this.isProjectRoot
  });

  factory ServerEntry.fromJson(Map<String, dynamic> json) {
    return ServerEntry(
      name: json['name']?.toString(),
      kind: json['kind']?.toString(),
      path: json['path']?.toString(),
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
  final String? nodeId;
  final String? path;
  final String? content;
  final String? size;

  ServerFileContent({
    this.nodeId,
    this.path,
    this.content,
    this.size
  });

  factory ServerFileContent.fromJson(Map<String, dynamic> json) {
    return ServerFileContent(
      nodeId: json['nodeId']?.toString(),
      path: json['path']?.toString(),
      content: json['content']?.toString(),
      size: json['size']?.toString()
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
  final String? nodeId;
  final String? path;
  final String? projectType;
  final List<ServerProjectOperation>? operations;

  ServerProjectOperations({
    this.nodeId,
    this.path,
    this.projectType,
    this.operations
  });

  factory ServerProjectOperations.fromJson(Map<String, dynamic> json) {
    return ServerProjectOperations(
      nodeId: json['nodeId']?.toString(),
      path: json['path']?.toString(),
      projectType: json['projectType']?.toString(),
      operations: (() {
        final list = _sdkworkAsList(json['operations']);
        if (list == null) {
          return null;
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
      'operations': operations?.map((item) => item.toJson()).toList(),
    };
  }
}

class ServerProjectOperation {
  final String? id;
  final String? kind;
  final String? label;
  final String? permission;
  final String? description;
  final bool? dangerous;

  ServerProjectOperation({
    this.id,
    this.kind,
    this.label,
    this.permission,
    this.description,
    this.dangerous
  });

  factory ServerProjectOperation.fromJson(Map<String, dynamic> json) {
    return ServerProjectOperation(
      id: json['id']?.toString(),
      kind: json['kind']?.toString(),
      label: json['label']?.toString(),
      permission: json['permission']?.toString(),
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
  final String? path;
  final String? operationId;

  ServerRunOperationRequest({
    this.path,
    this.operationId
  });

  factory ServerRunOperationRequest.fromJson(Map<String, dynamic> json) {
    return ServerRunOperationRequest(
      path: json['path']?.toString(),
      operationId: json['operationId']?.toString()
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
  final String? operationId;
  final int? exitCode;
  final String? stdout;
  final String? stderr;

  ServerOperationResult({
    this.operationId,
    this.exitCode,
    this.stdout,
    this.stderr
  });

  factory ServerOperationResult.fromJson(Map<String, dynamic> json) {
    return ServerOperationResult(
      operationId: json['operationId']?.toString(),
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
  final String? certificateId;
  final String? fingerprint;
  final String? syncVersion;
  final String? state;
  final String? observedAt;
  final String? failureCode;

  AgentCertificateObservation({
    this.certificateId,
    this.fingerprint,
    this.syncVersion,
    this.state,
    this.observedAt,
    this.failureCode
  });

  factory AgentCertificateObservation.fromJson(Map<String, dynamic> json) {
    return AgentCertificateObservation(
      certificateId: json['certificateId']?.toString(),
      fingerprint: json['fingerprint']?.toString(),
      syncVersion: json['syncVersion']?.toString(),
      state: json['state']?.toString(),
      observedAt: json['observedAt']?.toString(),
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
  final String? serverId;
  final int? status;
  final String? acknowledgedAt;

  AgentHeartbeatResponse({
    this.serverId,
    this.status,
    this.acknowledgedAt
  });

  factory AgentHeartbeatResponse.fromJson(Map<String, dynamic> json) {
    return AgentHeartbeatResponse(
      serverId: json['serverId']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      acknowledgedAt: json['acknowledgedAt']?.toString()
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
  final String? serverId;
  final String? syncVersion;
  final bool? unchanged;
  final List<AgentNginxConfigBundle>? nginxConfigs;
  final List<AgentCertificateBundle>? certificates;

  AgentSyncResponse({
    this.serverId,
    this.syncVersion,
    this.unchanged,
    this.nginxConfigs,
    this.certificates
  });

  factory AgentSyncResponse.fromJson(Map<String, dynamic> json) {
    return AgentSyncResponse(
      serverId: json['serverId']?.toString(),
      syncVersion: json['syncVersion']?.toString(),
      unchanged: json['unchanged'] is bool ? json['unchanged'] : null,
      nginxConfigs: (() {
        final list = _sdkworkAsList(json['nginxConfigs']);
        if (list == null) {
          return null;
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
          return null;
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
      'nginxConfigs': nginxConfigs?.map((item) => item.toJson()).toList(),
      'certificates': certificates?.map((item) => item.toJson()).toList(),
    };
  }
}

class AgentNginxConfigBundle {
  final String? configId;
  final String? domain;
  final String? configContent;
  final String? fingerprint;
  final String? version;

  AgentNginxConfigBundle({
    this.configId,
    this.domain,
    this.configContent,
    this.fingerprint,
    this.version
  });

  factory AgentNginxConfigBundle.fromJson(Map<String, dynamic> json) {
    return AgentNginxConfigBundle(
      configId: json['configId']?.toString(),
      domain: json['domain']?.toString(),
      configContent: json['configContent']?.toString(),
      fingerprint: json['fingerprint']?.toString(),
      version: json['version']?.toString()
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
  final String? certificateId;
  final String? certName;
  final String? fingerprint;
  final List<String>? hostnames;
  final String? fullchainPem;
  final String? privkeyPem;

  AgentCertificateBundle({
    this.certificateId,
    this.certName,
    this.fingerprint,
    this.hostnames,
    this.fullchainPem,
    this.privkeyPem
  });

  factory AgentCertificateBundle.fromJson(Map<String, dynamic> json) {
    return AgentCertificateBundle(
      certificateId: json['certificateId']?.toString(),
      certName: json['certName']?.toString(),
      fingerprint: json['fingerprint']?.toString(),
      hostnames: (() {
        final list = _sdkworkAsList(json['hostnames']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      fullchainPem: json['fullchainPem']?.toString(),
      privkeyPem: json['privkeyPem']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'certificateId': certificateId,
      'certName': certName,
      'fingerprint': fingerprint,
      'hostnames': hostnames?.map((item) => item).toList(),
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
  final int? code;
  final dynamic data;
  final String? traceId;

  SdkWorkApiResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory SdkWorkApiResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkApiResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final Map<String, dynamic>? item;

  SdkWorkResourceData({
    this.item
  });

  factory SdkWorkResourceData.fromJson(Map<String, dynamic> json) {
    return SdkWorkResourceData(
      item: _sdkworkAsMap(json['item'])
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'item': item,
    };
  }
}

class SdkWorkPageData {
  final List<Map<String, dynamic>>? items;
  final PageInfo? pageInfo;

  SdkWorkPageData({
    this.items,
    this.pageInfo
  });

  factory SdkWorkPageData.fromJson(Map<String, dynamic> json) {
    return SdkWorkPageData(
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => _sdkworkAsMap(item))
            .whereType<Map<String, dynamic>>()
            .toList();
      })(),
      pageInfo: (() {
        final map = _sdkworkAsMap(json['pageInfo']);
        return map == null ? null : PageInfo.fromJson(map);
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'items': items?.map((item) => item).toList(),
      'pageInfo': pageInfo?.toJson(),
    };
  }
}

class SdkWorkCommandData {
  final bool? accepted;
  final String? resourceId;
  final String? status;

  SdkWorkCommandData({
    this.accepted,
    this.resourceId,
    this.status
  });

  factory SdkWorkCommandData.fromJson(Map<String, dynamic> json) {
    return SdkWorkCommandData(
      accepted: json['accepted'] is bool ? json['accepted'] : null,
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
  final bool? accepted;
  final String? operationId;
  final String? status;
  final String? pollUrl;

  SdkWorkAsyncData({
    this.accepted,
    this.operationId,
    this.status,
    this.pollUrl
  });

  factory SdkWorkAsyncData.fromJson(Map<String, dynamic> json) {
    return SdkWorkAsyncData(
      accepted: json['accepted'] is bool ? json['accepted'] : null,
      operationId: json['operationId']?.toString(),
      status: json['status']?.toString(),
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
  final String? mode;
  final int? page;
  final int? pageSize;
  final String? totalItems;
  final int? totalPages;
  final String? nextCursor;
  final bool? hasMore;

  PageInfo({
    this.mode,
    this.page,
    this.pageSize,
    this.totalItems,
    this.totalPages,
    this.nextCursor,
    this.hasMore
  });

  factory PageInfo.fromJson(Map<String, dynamic> json) {
    return PageInfo(
      mode: json['mode']?.toString(),
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
  final String? field;
  final String? message;
  final int? code;

  FieldError({
    this.field,
    this.message,
    this.code
  });

  factory FieldError.fromJson(Map<String, dynamic> json) {
    return FieldError(
      field: json['field']?.toString(),
      message: json['message']?.toString(),
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
  final int? code;
  final dynamic data;
  final String? traceId;

  SdkWorkResourceResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory SdkWorkResourceResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkResourceResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  SdkWorkListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory SdkWorkListResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  SdkWorkCommandResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory SdkWorkCommandResponse.fromJson(Map<String, dynamic> json) {
    return SdkWorkCommandResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsRetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsUpdateResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsActivateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsActivateResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsActivateResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsPauseResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsPauseResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsPauseResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDomainsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDomainsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDomainsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDomainsVerifyResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDomainsVerifyResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsVerifyResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDomainsListenerCertificateBindingsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDomainsListenerCertificateBindingsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsListenerCertificateBindingsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDomainsListenerCertificateBindingsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDomainsListenerCertificateBindingsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsListenerCertificateBindingsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  RootDomainsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory RootDomainsListResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  RootDomainsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory RootDomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return RootDomainsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  RootDomainsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory RootDomainsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainsRetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  RootDomainsSubdomainsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory RootDomainsSubdomainsListResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainsSubdomainsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  RootDomainsSubdomainsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory RootDomainsSubdomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return RootDomainsSubdomainsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  DomainsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory DomainsListResponse.fromJson(Map<String, dynamic> json) {
    return DomainsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  DomainsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory DomainsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return DomainsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  DomainsVerifyResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory DomainsVerifyResponse.fromJson(Map<String, dynamic> json) {
    return DomainsVerifyResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  DomainsApplicationBindingUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory DomainsApplicationBindingUpdateResponse.fromJson(Map<String, dynamic> json) {
    return DomainsApplicationBindingUpdateResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsSourceVersionsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsSourceVersionsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsSourceVersionsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsSourceVersionsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsSourceVersionsGitImportCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsSourceVersionsGitImportCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsGitImportCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsSourceVersionsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsSourceVersionsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsSourceVersionsRetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDeploymentsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDeploymentsListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDeploymentsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDeploymentsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDeploymentsRollbackResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDeploymentsRollbackResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsRollbackResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  CertificatesListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory CertificatesListResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  CertificatesIssueResponse202({
    this.code,
    this.data,
    this.traceId
  });

  factory CertificatesIssueResponse202.fromJson(Map<String, dynamic> json) {
    return CertificatesIssueResponse202(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  CertificatesOperationsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory CertificatesOperationsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesOperationsRetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  CertificatesUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory CertificatesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesUpdateResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  CertificatesRenewResponse202({
    this.code,
    this.data,
    this.traceId
  });

  factory CertificatesRenewResponse202.fromJson(Map<String, dynamic> json) {
    return CertificatesRenewResponse202(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  CertificatesRevokeResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory CertificatesRevokeResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesRevokeResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  CertificatesDistributionListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory CertificatesDistributionListResponse.fromJson(Map<String, dynamic> json) {
    return CertificatesDistributionListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ConfigsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ConfigsListResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ConfigsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ConfigsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ConfigsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ConfigsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ConfigsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsRetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ConfigsUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ConfigsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsUpdateResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ConfigsValidateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ConfigsValidateResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsValidateResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ConfigsDeployResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ConfigsDeployResponse.fromJson(Map<String, dynamic> json) {
    return ConfigsDeployResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ReloadResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ReloadResponse.fromJson(Map<String, dynamic> json) {
    return ReloadResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  StatusRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory StatusRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return StatusRetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ServersListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ServersListResponse.fromJson(Map<String, dynamic> json) {
    return ServersListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ServersCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ServersCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ServersCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ServerFilesNodesListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ServerFilesNodesListResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodesListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ServerFilesNodeDirectoryListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ServerFilesNodeDirectoryListResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeDirectoryListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ServerFilesNodeRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ServerFilesNodeRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeRetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ServerFilesNodeOperationsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ServerFilesNodeOperationsListResponse.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeOperationsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  ServerFilesNodeOperationsCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ServerFilesNodeOperationsCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ServerFilesNodeOperationsCreateResponse201(
      code: json['code'] is int ? json['code'] : null,
      data: json['data'],
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  HeartbeatResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory HeartbeatResponse.fromJson(Map<String, dynamic> json) {
    return HeartbeatResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  RetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory RetrieveResponse.fromJson(Map<String, dynamic> json) {
    return RetrieveResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
  final int? code;
  final dynamic data;
  final String? traceId;

  AuditLogsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory AuditLogsListResponse.fromJson(Map<String, dynamic> json) {
    return AuditLogsListResponse(
      code: json['code'] is int ? json['code'] : null,
      data: _sdkworkAsMap(json['data']),
      traceId: json['traceId']?.toString()
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
