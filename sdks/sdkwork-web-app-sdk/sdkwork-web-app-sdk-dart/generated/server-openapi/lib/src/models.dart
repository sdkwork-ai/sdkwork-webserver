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
  final String? applicationType;
  final int? siteType;
  final Map<String, dynamic>? runtimeConfig;
  final ApplicationStoreListing? storeListing;

  CreateApplicationRequest({
    this.name,
    this.slug,
    this.description,
    this.applicationType,
    this.siteType,
    this.runtimeConfig,
    this.storeListing
  });

  factory CreateApplicationRequest.fromJson(Map<String, dynamic> json) {
    return CreateApplicationRequest(
      name: json['name']?.toString(),
      slug: json['slug']?.toString(),
      description: json['description']?.toString(),
      applicationType: json['applicationType']?.toString(),
      siteType: json['siteType'] is int ? json['siteType'] : null,
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
      'applicationType': applicationType,
      'siteType': siteType,
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
  final String? applicationType;
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
    this.applicationType,
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
      applicationType: json['applicationType']?.toString(),
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
      'applicationType': applicationType,
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
  final String? hostname;
  final bool? isPrimary;
  final bool? sslEnabled;
  final String? sslProvider;

  CreateDomainRequest({
    this.hostname,
    this.isPrimary,
    this.sslEnabled,
    this.sslProvider
  });

  factory CreateDomainRequest.fromJson(Map<String, dynamic> json) {
    return CreateDomainRequest(
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

class DomainResponse {
  final String? id;
  final String? hostname;
  final String? applicationId;
  final String? applicationName;
  final String? certificateCount;
  final bool? isPrimary;
  final bool? isVerified;
  final bool? sslEnabled;
  final String? sslProvider;
  final int? status;
  final String? createdAt;

  DomainResponse({
    this.id,
    this.hostname,
    this.applicationId,
    this.applicationName,
    this.certificateCount,
    this.isPrimary,
    this.isVerified,
    this.sslEnabled,
    this.sslProvider,
    this.status,
    this.createdAt
  });

  factory DomainResponse.fromJson(Map<String, dynamic> json) {
    return DomainResponse(
      id: json['id']?.toString(),
      hostname: json['hostname']?.toString(),
      applicationId: json['applicationId']?.toString(),
      applicationName: json['applicationName']?.toString(),
      certificateCount: json['certificateCount']?.toString(),
      isPrimary: json['isPrimary'] is bool ? json['isPrimary'] : null,
      isVerified: json['isVerified'] is bool ? json['isVerified'] : null,
      sslEnabled: json['sslEnabled'] is bool ? json['sslEnabled'] : null,
      sslProvider: json['sslProvider']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      createdAt: json['createdAt']?.toString()
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

class SourceVersionConfigSnapshot {
  final String? appConfigPath;
  final String? deploymentConfigPath;
  final bool? appConfigDetected;
  final bool? deploymentConfigDetected;

  SourceVersionConfigSnapshot({
    this.appConfigPath,
    this.deploymentConfigPath,
    this.appConfigDetected,
    this.deploymentConfigDetected
  });

  factory SourceVersionConfigSnapshot.fromJson(Map<String, dynamic> json) {
    return SourceVersionConfigSnapshot(
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

class CreateSourceVersionRequest {
  final String? versionTag;
  final String? sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final SourceVersionConfigSnapshot? configSnapshot;

  CreateSourceVersionRequest({
    this.versionTag,
    this.sourceType,
    this.sourceRef,
    this.commitHash,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash,
    this.configSnapshot
  });

  factory CreateSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return CreateSourceVersionRequest(
      versionTag: json['versionTag']?.toString(),
      sourceType: json['sourceType']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
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
  final String? versionTag;
  final String? repositoryUrl;
  final String? gitRef;

  ImportGitSourceVersionRequest({
    this.versionTag,
    this.repositoryUrl,
    this.gitRef
  });

  factory ImportGitSourceVersionRequest.fromJson(Map<String, dynamic> json) {
    return ImportGitSourceVersionRequest(
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

class SourceVersionResponse {
  final String? id;
  final String? applicationId;
  final String? versionTag;
  final String? sourceType;
  final String? sourceRef;
  final String? commitHash;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final SourceVersionConfigSnapshot? configSnapshot;
  final int? status;
  final bool? retained;
  final String? createdAt;

  SourceVersionResponse({
    this.id,
    this.applicationId,
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

  factory SourceVersionResponse.fromJson(Map<String, dynamic> json) {
    return SourceVersionResponse(
      id: json['id']?.toString(),
      applicationId: json['applicationId']?.toString(),
      versionTag: json['versionTag']?.toString(),
      sourceType: json['sourceType']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      commitHash: json['commitHash']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
      configSnapshot: (() {
        final map = _sdkworkAsMap(json['configSnapshot']);
        return map == null ? null : SourceVersionConfigSnapshot.fromJson(map);
      })(),
      status: json['status'] is int ? json['status'] : null,
      retained: json['retained'] is bool ? json['retained'] : null,
      createdAt: json['createdAt']?.toString()
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
      'configSnapshot': configSnapshot?.toJson(),
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
  final int? deployType;
  final String? versionTag;
  final String? commitHash;
  final String? sourceRef;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final String? environment;

  CreateDeploymentRequest({
    this.sourceVersionId,
    this.deployType,
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
      deployType: json['deployType'] is int ? json['deployType'] : null,
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
  final String? id;
  final String? applicationId;
  final int? deployType;
  final String? sourceVersionId;
  final String? versionTag;
  final String? commitHash;
  final String? sourceRef;
  final String? rollbackFromDeploymentId;
  final String? environment;
  final String? artifactDriveUri;
  final String? artifactSize;
  final String? artifactHash;
  final int? status;
  final String? startedAt;
  final String? completedAt;
  final String? durationMs;
  final String? createdAt;

  DeploymentResponse({
    this.id,
    this.applicationId,
    this.deployType,
    this.sourceVersionId,
    this.versionTag,
    this.commitHash,
    this.sourceRef,
    this.rollbackFromDeploymentId,
    this.environment,
    this.artifactDriveUri,
    this.artifactSize,
    this.artifactHash,
    this.status,
    this.startedAt,
    this.completedAt,
    this.durationMs,
    this.createdAt
  });

  factory DeploymentResponse.fromJson(Map<String, dynamic> json) {
    return DeploymentResponse(
      id: json['id']?.toString(),
      applicationId: json['applicationId']?.toString(),
      deployType: json['deployType'] is int ? json['deployType'] : null,
      sourceVersionId: json['sourceVersionId']?.toString(),
      versionTag: json['versionTag']?.toString(),
      commitHash: json['commitHash']?.toString(),
      sourceRef: json['sourceRef']?.toString(),
      rollbackFromDeploymentId: json['rollbackFromDeploymentId']?.toString(),
      environment: json['environment']?.toString(),
      artifactDriveUri: json['artifactDriveUri']?.toString(),
      artifactSize: json['artifactSize']?.toString(),
      artifactHash: json['artifactHash']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      startedAt: json['startedAt']?.toString(),
      completedAt: json['completedAt']?.toString(),
      durationMs: json['durationMs']?.toString(),
      createdAt: json['createdAt']?.toString()
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
  final String? key;
  final String? value;
  final String? environment;
  final bool? isSecret;

  CreateEnvVariableRequest({
    this.key,
    this.value,
    this.environment,
    this.isSecret
  });

  factory CreateEnvVariableRequest.fromJson(Map<String, dynamic> json) {
    return CreateEnvVariableRequest(
      key: json['key']?.toString(),
      value: json['value']?.toString(),
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
  final String? value;
  final bool? isSecret;

  UpdateEnvVariableRequest({
    this.value,
    this.isSecret
  });

  factory UpdateEnvVariableRequest.fromJson(Map<String, dynamic> json) {
    return UpdateEnvVariableRequest(
      value: json['value']?.toString(),
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
  final String? applicationId;
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
    this.applicationId,
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
      applicationId: json['applicationId']?.toString(),
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
      'applicationId': applicationId,
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

class CreateHealthCheckRequest {
  final int? checkType;
  final String? checkUrl;
  final int? checkInterval;
  final int? timeoutMs;
  final int? retryCount;

  CreateHealthCheckRequest({
    this.checkType,
    this.checkUrl,
    this.checkInterval,
    this.timeoutMs,
    this.retryCount
  });

  factory CreateHealthCheckRequest.fromJson(Map<String, dynamic> json) {
    return CreateHealthCheckRequest(
      checkType: json['checkType'] is int ? json['checkType'] : null,
      checkUrl: json['checkUrl']?.toString(),
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
  final String? id;
  final int? checkType;
  final String? checkUrl;
  final int? checkInterval;
  final int? timeoutMs;
  final int? retryCount;
  final int? status;
  final String? createdAt;

  HealthCheckResponse({
    this.id,
    this.checkType,
    this.checkUrl,
    this.checkInterval,
    this.timeoutMs,
    this.retryCount,
    this.status,
    this.createdAt
  });

  factory HealthCheckResponse.fromJson(Map<String, dynamic> json) {
    return HealthCheckResponse(
      id: json['id']?.toString(),
      checkType: json['checkType'] is int ? json['checkType'] : null,
      checkUrl: json['checkUrl']?.toString(),
      checkInterval: json['checkInterval'] is int ? json['checkInterval'] : null,
      timeoutMs: json['timeoutMs'] is int ? json['timeoutMs'] : null,
      retryCount: json['retryCount'] is int ? json['retryCount'] : null,
      status: json['status'] is int ? json['status'] : null,
      createdAt: json['createdAt']?.toString()
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

class ApplicationsDomainsRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDomainsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDomainsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDomainsRetrieveResponse(
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

class ApplicationsDeploymentsRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsDeploymentsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsDeploymentsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsDeploymentsRetrieveResponse(
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

class ApplicationsEnvVariablesListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsEnvVariablesListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsEnvVariablesListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsEnvVariablesListResponse(
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

class ApplicationsEnvVariablesCreateResponse201 {
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsEnvVariablesCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsEnvVariablesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsEnvVariablesCreateResponse201(
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

class ApplicationsEnvVariablesUpdateResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsEnvVariablesUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsEnvVariablesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsEnvVariablesUpdateResponse(
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

class ApplicationsHealthChecksListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsHealthChecksListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsHealthChecksListResponse.fromJson(Map<String, dynamic> json) {
    return ApplicationsHealthChecksListResponse(
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

class ApplicationsHealthChecksCreateResponse201 {
  final int? code;
  final dynamic data;
  final String? traceId;

  ApplicationsHealthChecksCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ApplicationsHealthChecksCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ApplicationsHealthChecksCreateResponse201(
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
