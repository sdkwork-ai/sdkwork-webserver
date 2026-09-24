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

class ClusterHeartbeatSampleResponse {
  final String? id;
  final int? status;
  final int? latencyMs;
  final Map<String, dynamic>? metrics;
  final String? reportedAt;

  ClusterHeartbeatSampleResponse({
    this.id,
    this.status,
    this.latencyMs,
    this.metrics,
    this.reportedAt
  });

  factory ClusterHeartbeatSampleResponse.fromJson(Map<String, dynamic> json) {
    return ClusterHeartbeatSampleResponse(
      id: json['id']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      latencyMs: json['latencyMs'] is int ? json['latencyMs'] : null,
      metrics: _sdkworkAsMap(json['metrics']),
      reportedAt: json['reportedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'status': status,
      'latencyMs': latencyMs,
      'metrics': metrics,
      'reportedAt': reportedAt,
    };
  }
}

class PublishClusterSyncRequest {
  final String? kind;
  final Map<String, dynamic>? payload;

  PublishClusterSyncRequest({
    this.kind,
    this.payload
  });

  factory PublishClusterSyncRequest.fromJson(Map<String, dynamic> json) {
    return PublishClusterSyncRequest(
      kind: json['kind']?.toString(),
      payload: _sdkworkAsMap(json['payload'])
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'kind': kind,
      'payload': payload,
    };
  }
}

class ClusterSyncManifest {
  final String? clusterId;
  final String? kind;
  final String? revision;
  final String? sha256;
  final Map<String, dynamic>? payload;
  final String? createdAt;

  ClusterSyncManifest({
    this.clusterId,
    this.kind,
    this.revision,
    this.sha256,
    this.payload,
    this.createdAt
  });

  factory ClusterSyncManifest.fromJson(Map<String, dynamic> json) {
    return ClusterSyncManifest(
      clusterId: json['clusterId']?.toString(),
      kind: json['kind']?.toString(),
      revision: json['revision']?.toString(),
      sha256: json['sha256']?.toString(),
      payload: _sdkworkAsMap(json['payload']),
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'clusterId': clusterId,
      'kind': kind,
      'revision': revision,
      'sha256': sha256,
      'payload': payload,
      'createdAt': createdAt,
    };
  }
}

class ProbeClusterInstanceRequest {
  final String? path;
  final int? timeoutMs;

  ProbeClusterInstanceRequest({
    this.path,
    this.timeoutMs
  });

  factory ProbeClusterInstanceRequest.fromJson(Map<String, dynamic> json) {
    return ProbeClusterInstanceRequest(
      path: json['path']?.toString(),
      timeoutMs: json['timeoutMs'] is int ? json['timeoutMs'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'path': path,
      'timeoutMs': timeoutMs,
    };
  }
}

class ClusterProbeRunResponse {
  final bool? healthy;
  final int? latencyMs;
  final int? failures;
  final bool? ejected;
  final bool? recovered;

  ClusterProbeRunResponse({
    this.healthy,
    this.latencyMs,
    this.failures,
    this.ejected,
    this.recovered
  });

  factory ClusterProbeRunResponse.fromJson(Map<String, dynamic> json) {
    return ClusterProbeRunResponse(
      healthy: json['healthy'] is bool ? json['healthy'] : null,
      latencyMs: json['latencyMs'] is int ? json['latencyMs'] : null,
      failures: json['failures'] is int ? json['failures'] : null,
      ejected: json['ejected'] is bool ? json['ejected'] : null,
      recovered: json['recovered'] is bool ? json['recovered'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'healthy': healthy,
      'latencyMs': latencyMs,
      'failures': failures,
      'ejected': ejected,
      'recovered': recovered,
    };
  }
}

class EnqueueClusterPeerMessagesRequest {
  final String? clusterId;
  final String? toInstanceId;
  final String? fromInstanceId;
  final String? messageType;
  final Map<String, dynamic>? payload;
  final int? expiresInSeconds;

  EnqueueClusterPeerMessagesRequest({
    this.clusterId,
    this.toInstanceId,
    this.fromInstanceId,
    this.messageType,
    this.payload,
    this.expiresInSeconds
  });

  factory EnqueueClusterPeerMessagesRequest.fromJson(Map<String, dynamic> json) {
    return EnqueueClusterPeerMessagesRequest(
      clusterId: json['clusterId']?.toString(),
      toInstanceId: json['toInstanceId']?.toString(),
      fromInstanceId: json['fromInstanceId']?.toString(),
      messageType: json['messageType']?.toString(),
      payload: _sdkworkAsMap(json['payload']),
      expiresInSeconds: json['expiresInSeconds'] is int ? json['expiresInSeconds'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'clusterId': clusterId,
      'toInstanceId': toInstanceId,
      'fromInstanceId': fromInstanceId,
      'messageType': messageType,
      'payload': payload,
      'expiresInSeconds': expiresInSeconds,
    };
  }
}

class EnqueueClusterPeerMessagesResponse {
  final String? enqueued;

  EnqueueClusterPeerMessagesResponse({
    this.enqueued
  });

  factory EnqueueClusterPeerMessagesResponse.fromJson(Map<String, dynamic> json) {
    return EnqueueClusterPeerMessagesResponse(
      enqueued: json['enqueued']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'enqueued': enqueued,
    };
  }
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
  final bool? hasSourceVersion;
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
    this.hasSourceVersion,
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
      hasSourceVersion: json['hasSourceVersion'] is bool ? json['hasSourceVersion'] : null,
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
      'hasSourceVersion': hasSourceVersion,
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

class UpdateRootDomainRequest {
  final String? displayName;
  final String? dnsProvider;
  final String? providerZoneRef;
  final int? status;

  UpdateRootDomainRequest({
    this.displayName,
    this.dnsProvider,
    this.providerZoneRef,
    this.status
  });

  factory UpdateRootDomainRequest.fromJson(Map<String, dynamic> json) {
    return UpdateRootDomainRequest(
      displayName: json['displayName']?.toString(),
      dnsProvider: json['dnsProvider']?.toString(),
      providerZoneRef: json['providerZoneRef']?.toString(),
      status: json['status'] is int ? json['status'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'displayName': displayName,
      'dnsProvider': dnsProvider,
      'providerZoneRef': providerZoneRef,
      'status': status,
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
  final String? displayName;
  final String? dnsProvider;
  final String? providerZoneRef;
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
    this.displayName,
    this.dnsProvider,
    this.providerZoneRef,
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
      displayName: json['displayName']?.toString(),
      dnsProvider: json['dnsProvider']?.toString(),
      providerZoneRef: json['providerZoneRef']?.toString(),
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
      'displayName': displayName,
      'dnsProvider': dnsProvider,
      'providerZoneRef': providerZoneRef,
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
  final String? failureDetail;
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
    this.failureDetail,
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
      failureDetail: json['failureDetail']?.toString(),
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
      'failureDetail': failureDetail,
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

class WebserverConfigCatalog {
  final String? configRoot;
  final List<WebserverConfigEntry>? items;

  WebserverConfigCatalog({
    this.configRoot,
    this.items
  });

  factory WebserverConfigCatalog.fromJson(Map<String, dynamic> json) {
    return WebserverConfigCatalog(
      configRoot: json['configRoot']?.toString(),
      items: (() {
        final list = _sdkworkAsList(json['items']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : WebserverConfigEntry.fromJson(map);
      })())
            .whereType<WebserverConfigEntry>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'configRoot': configRoot,
      'items': items?.map((item) => item.toJson()).toList(),
    };
  }
}

class WebserverConfigEntry {
  final String? id;
  final String? kind;
  final String? name;
  final String? path;
  final String? language;
  final String? size;
  final String? updatedAt;
  final bool? writable;

  WebserverConfigEntry({
    this.id,
    this.kind,
    this.name,
    this.path,
    this.language,
    this.size,
    this.updatedAt,
    this.writable
  });

  factory WebserverConfigEntry.fromJson(Map<String, dynamic> json) {
    return WebserverConfigEntry(
      id: json['id']?.toString(),
      kind: json['kind']?.toString(),
      name: json['name']?.toString(),
      path: json['path']?.toString(),
      language: json['language']?.toString(),
      size: json['size']?.toString(),
      updatedAt: json['updatedAt']?.toString(),
      writable: json['writable'] is bool ? json['writable'] : null
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'kind': kind,
      'name': name,
      'path': path,
      'language': language,
      'size': size,
      'updatedAt': updatedAt,
      'writable': writable,
    };
  }
}

class WebserverConfigFile {
  final String? id;
  final String? kind;
  final String? name;
  final String? path;
  final String? language;
  final bool? writable;
  final String? content;
  final String? size;
  final String? sha256;
  final String? updatedAt;

  WebserverConfigFile({
    this.id,
    this.kind,
    this.name,
    this.path,
    this.language,
    this.writable,
    this.content,
    this.size,
    this.sha256,
    this.updatedAt
  });

  factory WebserverConfigFile.fromJson(Map<String, dynamic> json) {
    return WebserverConfigFile(
      id: json['id']?.toString(),
      kind: json['kind']?.toString(),
      name: json['name']?.toString(),
      path: json['path']?.toString(),
      language: json['language']?.toString(),
      writable: json['writable'] is bool ? json['writable'] : null,
      content: json['content']?.toString(),
      size: json['size']?.toString(),
      sha256: json['sha256']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'kind': kind,
      'name': name,
      'path': path,
      'language': language,
      'writable': writable,
      'content': content,
      'size': size,
      'sha256': sha256,
      'updatedAt': updatedAt,
    };
  }
}

class WebserverConfigWriteRequest {
  final String? content;
  final String? expectedSha256;

  WebserverConfigWriteRequest({
    this.content,
    this.expectedSha256
  });

  factory WebserverConfigWriteRequest.fromJson(Map<String, dynamic> json) {
    return WebserverConfigWriteRequest(
      content: json['content']?.toString(),
      expectedSha256: json['expectedSha256']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'content': content,
      'expectedSha256': expectedSha256,
    };
  }
}

class WebserverConfigWriteResult {
  final String? id;
  final String? path;
  final String? size;
  final String? sha256;
  final String? backupPath;
  final String? updatedAt;

  WebserverConfigWriteResult({
    this.id,
    this.path,
    this.size,
    this.sha256,
    this.backupPath,
    this.updatedAt
  });

  factory WebserverConfigWriteResult.fromJson(Map<String, dynamic> json) {
    return WebserverConfigWriteResult(
      id: json['id']?.toString(),
      path: json['path']?.toString(),
      size: json['size']?.toString(),
      sha256: json['sha256']?.toString(),
      backupPath: json['backupPath']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'path': path,
      'size': size,
      'sha256': sha256,
      'backupPath': backupPath,
      'updatedAt': updatedAt,
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
  final bool? timedOut;
  final String? stdout;
  final String? stderr;
  final bool? stdoutTruncated;
  final bool? stderrTruncated;
  final int? pid;
  final String? pidFile;
  final String? logFile;
  final bool? stopped;
  final String? message;

  ServerOperationResult({
    this.operationId,
    this.exitCode,
    this.timedOut,
    this.stdout,
    this.stderr,
    this.stdoutTruncated,
    this.stderrTruncated,
    this.pid,
    this.pidFile,
    this.logFile,
    this.stopped,
    this.message
  });

  factory ServerOperationResult.fromJson(Map<String, dynamic> json) {
    return ServerOperationResult(
      operationId: json['operationId']?.toString(),
      exitCode: json['exitCode'] is int ? json['exitCode'] : null,
      timedOut: json['timedOut'] is bool ? json['timedOut'] : null,
      stdout: json['stdout']?.toString(),
      stderr: json['stderr']?.toString(),
      stdoutTruncated: json['stdoutTruncated'] is bool ? json['stdoutTruncated'] : null,
      stderrTruncated: json['stderrTruncated'] is bool ? json['stderrTruncated'] : null,
      pid: json['pid'] is int ? json['pid'] : null,
      pidFile: json['pidFile']?.toString(),
      logFile: json['logFile']?.toString(),
      stopped: json['stopped'] is bool ? json['stopped'] : null,
      message: json['message']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'operationId': operationId,
      'exitCode': exitCode,
      'timedOut': timedOut,
      'stdout': stdout,
      'stderr': stderr,
      'stdoutTruncated': stdoutTruncated,
      'stderrTruncated': stderrTruncated,
      'pid': pid,
      'pidFile': pidFile,
      'logFile': logFile,
      'stopped': stopped,
      'message': message,
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

class ClusterResponse {
  final String? id;
  final String? name;
  final String? code;
  final String? description;
  final int? status;
  final int? heartbeatIntervalSeconds;
  final int? offlineThresholdSeconds;
  final String? hostCount;
  final String? instanceCount;
  final String? onlineInstanceCount;
  final String? lbStrategy;
  final List<String>? servedDomains;
  final String? createdAt;
  final String? updatedAt;

  ClusterResponse({
    this.id,
    this.name,
    this.code,
    this.description,
    this.status,
    this.heartbeatIntervalSeconds,
    this.offlineThresholdSeconds,
    this.hostCount,
    this.instanceCount,
    this.onlineInstanceCount,
    this.lbStrategy,
    this.servedDomains,
    this.createdAt,
    this.updatedAt
  });

  factory ClusterResponse.fromJson(Map<String, dynamic> json) {
    return ClusterResponse(
      id: json['id']?.toString(),
      name: json['name']?.toString(),
      code: json['code']?.toString(),
      description: json['description']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      heartbeatIntervalSeconds: json['heartbeatIntervalSeconds'] is int ? json['heartbeatIntervalSeconds'] : null,
      offlineThresholdSeconds: json['offlineThresholdSeconds'] is int ? json['offlineThresholdSeconds'] : null,
      hostCount: json['hostCount']?.toString(),
      instanceCount: json['instanceCount']?.toString(),
      onlineInstanceCount: json['onlineInstanceCount']?.toString(),
      lbStrategy: json['lbStrategy']?.toString(),
      servedDomains: (() {
        final list = _sdkworkAsList(json['servedDomains']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'name': name,
      'code': code,
      'description': description,
      'status': status,
      'heartbeatIntervalSeconds': heartbeatIntervalSeconds,
      'offlineThresholdSeconds': offlineThresholdSeconds,
      'hostCount': hostCount,
      'instanceCount': instanceCount,
      'onlineInstanceCount': onlineInstanceCount,
      'lbStrategy': lbStrategy,
      'servedDomains': servedDomains?.map((item) => item).toList(),
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class CreateClusterRequest {
  final String? name;
  final String? code;
  final String? description;
  final int? heartbeatIntervalSeconds;
  final int? offlineThresholdSeconds;
  final String? lbStrategy;
  final List<String>? servedDomains;

  CreateClusterRequest({
    this.name,
    this.code,
    this.description,
    this.heartbeatIntervalSeconds,
    this.offlineThresholdSeconds,
    this.lbStrategy,
    this.servedDomains
  });

  factory CreateClusterRequest.fromJson(Map<String, dynamic> json) {
    return CreateClusterRequest(
      name: json['name']?.toString(),
      code: json['code']?.toString(),
      description: json['description']?.toString(),
      heartbeatIntervalSeconds: json['heartbeatIntervalSeconds'] is int ? json['heartbeatIntervalSeconds'] : null,
      offlineThresholdSeconds: json['offlineThresholdSeconds'] is int ? json['offlineThresholdSeconds'] : null,
      lbStrategy: json['lbStrategy']?.toString(),
      servedDomains: (() {
        final list = _sdkworkAsList(json['servedDomains']);
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
      'name': name,
      'code': code,
      'description': description,
      'heartbeatIntervalSeconds': heartbeatIntervalSeconds,
      'offlineThresholdSeconds': offlineThresholdSeconds,
      'lbStrategy': lbStrategy,
      'servedDomains': servedDomains?.map((item) => item).toList(),
    };
  }
}

class UpdateClusterRequest {
  final String? name;
  final String? description;
  final int? status;
  final int? heartbeatIntervalSeconds;
  final int? offlineThresholdSeconds;
  final String? lbStrategy;
  final List<String>? servedDomains;

  UpdateClusterRequest({
    this.name,
    this.description,
    this.status,
    this.heartbeatIntervalSeconds,
    this.offlineThresholdSeconds,
    this.lbStrategy,
    this.servedDomains
  });

  factory UpdateClusterRequest.fromJson(Map<String, dynamic> json) {
    return UpdateClusterRequest(
      name: json['name']?.toString(),
      description: json['description']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      heartbeatIntervalSeconds: json['heartbeatIntervalSeconds'] is int ? json['heartbeatIntervalSeconds'] : null,
      offlineThresholdSeconds: json['offlineThresholdSeconds'] is int ? json['offlineThresholdSeconds'] : null,
      lbStrategy: json['lbStrategy']?.toString(),
      servedDomains: (() {
        final list = _sdkworkAsList(json['servedDomains']);
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
      'name': name,
      'description': description,
      'status': status,
      'heartbeatIntervalSeconds': heartbeatIntervalSeconds,
      'offlineThresholdSeconds': offlineThresholdSeconds,
      'lbStrategy': lbStrategy,
      'servedDomains': servedDomains?.map((item) => item).toList(),
    };
  }
}

class ClusterHostResponse {
  final String? id;
  final String? clusterId;
  final String? name;
  final String? hostname;
  final String? machineCode;
  final String? osName;
  final String? osVersion;
  final String? kernelVersion;
  final String? arch;
  final String? cpuModel;
  final int? cpuCores;
  final String? memoryTotalMb;
  final String? remoteIp;
  final List<String>? localIps;
  final List<String>? macAddresses;
  final String? daemonVersion;
  final int? status;
  final String? lastHeartbeatAt;
  final String? instanceCount;
  final String? joinMode;
  final String? tunnelRouteDomain;
  final String? createdAt;
  final String? updatedAt;

  ClusterHostResponse({
    this.id,
    this.clusterId,
    this.name,
    this.hostname,
    this.machineCode,
    this.osName,
    this.osVersion,
    this.kernelVersion,
    this.arch,
    this.cpuModel,
    this.cpuCores,
    this.memoryTotalMb,
    this.remoteIp,
    this.localIps,
    this.macAddresses,
    this.daemonVersion,
    this.status,
    this.lastHeartbeatAt,
    this.instanceCount,
    this.joinMode,
    this.tunnelRouteDomain,
    this.createdAt,
    this.updatedAt
  });

  factory ClusterHostResponse.fromJson(Map<String, dynamic> json) {
    return ClusterHostResponse(
      id: json['id']?.toString(),
      clusterId: json['clusterId']?.toString(),
      name: json['name']?.toString(),
      hostname: json['hostname']?.toString(),
      machineCode: json['machineCode']?.toString(),
      osName: json['osName']?.toString(),
      osVersion: json['osVersion']?.toString(),
      kernelVersion: json['kernelVersion']?.toString(),
      arch: json['arch']?.toString(),
      cpuModel: json['cpuModel']?.toString(),
      cpuCores: json['cpuCores'] is int ? json['cpuCores'] : null,
      memoryTotalMb: json['memoryTotalMb']?.toString(),
      remoteIp: json['remoteIp']?.toString(),
      localIps: (() {
        final list = _sdkworkAsList(json['localIps']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      macAddresses: (() {
        final list = _sdkworkAsList(json['macAddresses']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => item?.toString())
            .whereType<String>()
            .toList();
      })(),
      daemonVersion: json['daemonVersion']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      lastHeartbeatAt: json['lastHeartbeatAt']?.toString(),
      instanceCount: json['instanceCount']?.toString(),
      joinMode: json['joinMode']?.toString(),
      tunnelRouteDomain: json['tunnelRouteDomain']?.toString(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'clusterId': clusterId,
      'name': name,
      'hostname': hostname,
      'machineCode': machineCode,
      'osName': osName,
      'osVersion': osVersion,
      'kernelVersion': kernelVersion,
      'arch': arch,
      'cpuModel': cpuModel,
      'cpuCores': cpuCores,
      'memoryTotalMb': memoryTotalMb,
      'remoteIp': remoteIp,
      'localIps': localIps?.map((item) => item).toList(),
      'macAddresses': macAddresses?.map((item) => item).toList(),
      'daemonVersion': daemonVersion,
      'status': status,
      'lastHeartbeatAt': lastHeartbeatAt,
      'instanceCount': instanceCount,
      'joinMode': joinMode,
      'tunnelRouteDomain': tunnelRouteDomain,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class UpdateClusterHostRequest {
  final String? name;
  final String? clusterId;

  UpdateClusterHostRequest({
    this.name,
    this.clusterId
  });

  factory UpdateClusterHostRequest.fromJson(Map<String, dynamic> json) {
    return UpdateClusterHostRequest(
      name: json['name']?.toString(),
      clusterId: json['clusterId']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'clusterId': clusterId,
    };
  }
}

class ClusterInstanceResponse {
  final String? id;
  final String? clusterId;
  final String? hostId;
  final String? hostName;
  final String? name;
  final String? role;
  final String? environment;
  final int? processPid;
  final String? processStartedAt;
  final String? bindHost;
  final int? bindPort;
  final String? publicEndpoint;
  final String? buildVersion;
  final int? status;
  final String? healthState;
  final String? lastHeartbeatAt;
  final String? lastOnlineAt;
  final String? uptimeSeconds;
  final Map<String, dynamic>? metrics;
  final String? joinMode;
  final int? qualityScore;
  final String? desiredConfigRevision;
  final String? appliedConfigRevision;
  final String? desiredApplicationsRevision;
  final String? appliedApplicationsRevision;
  final String? syncStatus;
  final bool? routingEnabled;
  final bool? draining;
  final bool? ejected;
  final int? restartCount;
  final Map<String, String>? labels;
  final int? routingWeight;
  final String? maintenanceNote;
  final int? probeFailures;
  final String? probeUrl;
  final String? createdAt;
  final String? updatedAt;

  ClusterInstanceResponse({
    this.id,
    this.clusterId,
    this.hostId,
    this.hostName,
    this.name,
    this.role,
    this.environment,
    this.processPid,
    this.processStartedAt,
    this.bindHost,
    this.bindPort,
    this.publicEndpoint,
    this.buildVersion,
    this.status,
    this.healthState,
    this.lastHeartbeatAt,
    this.lastOnlineAt,
    this.uptimeSeconds,
    this.metrics,
    this.joinMode,
    this.qualityScore,
    this.desiredConfigRevision,
    this.appliedConfigRevision,
    this.desiredApplicationsRevision,
    this.appliedApplicationsRevision,
    this.syncStatus,
    this.routingEnabled,
    this.draining,
    this.ejected,
    this.restartCount,
    this.labels,
    this.routingWeight,
    this.maintenanceNote,
    this.probeFailures,
    this.probeUrl,
    this.createdAt,
    this.updatedAt
  });

  factory ClusterInstanceResponse.fromJson(Map<String, dynamic> json) {
    return ClusterInstanceResponse(
      id: json['id']?.toString(),
      clusterId: json['clusterId']?.toString(),
      hostId: json['hostId']?.toString(),
      hostName: json['hostName']?.toString(),
      name: json['name']?.toString(),
      role: json['role']?.toString(),
      environment: json['environment']?.toString(),
      processPid: json['processPid'] is int ? json['processPid'] : null,
      processStartedAt: json['processStartedAt']?.toString(),
      bindHost: json['bindHost']?.toString(),
      bindPort: json['bindPort'] is int ? json['bindPort'] : null,
      publicEndpoint: json['publicEndpoint']?.toString(),
      buildVersion: json['buildVersion']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      healthState: json['healthState']?.toString(),
      lastHeartbeatAt: json['lastHeartbeatAt']?.toString(),
      lastOnlineAt: json['lastOnlineAt']?.toString(),
      uptimeSeconds: json['uptimeSeconds']?.toString(),
      metrics: _sdkworkAsMap(json['metrics']),
      joinMode: json['joinMode']?.toString(),
      qualityScore: json['qualityScore'] is int ? json['qualityScore'] : null,
      desiredConfigRevision: json['desiredConfigRevision']?.toString(),
      appliedConfigRevision: json['appliedConfigRevision']?.toString(),
      desiredApplicationsRevision: json['desiredApplicationsRevision']?.toString(),
      appliedApplicationsRevision: json['appliedApplicationsRevision']?.toString(),
      syncStatus: json['syncStatus']?.toString(),
      routingEnabled: json['routingEnabled'] is bool ? json['routingEnabled'] : null,
      draining: json['draining'] is bool ? json['draining'] : null,
      ejected: json['ejected'] is bool ? json['ejected'] : null,
      restartCount: json['restartCount'] is int ? json['restartCount'] : null,
      labels: (() {
        final map = _sdkworkAsMap(json['labels']);
        if (map == null) {
          return null;
        }
        final result = <String, String>{};
        map.forEach((key, item) {
          final deserialized = item?.toString();
          if (deserialized is String) {
            result[key] = deserialized;
          }
        });
        return result;
      })(),
      routingWeight: json['routingWeight'] is int ? json['routingWeight'] : null,
      maintenanceNote: json['maintenanceNote']?.toString(),
      probeFailures: json['probeFailures'] is int ? json['probeFailures'] : null,
      probeUrl: json['probeUrl']?.toString(),
      createdAt: json['createdAt']?.toString(),
      updatedAt: json['updatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'clusterId': clusterId,
      'hostId': hostId,
      'hostName': hostName,
      'name': name,
      'role': role,
      'environment': environment,
      'processPid': processPid,
      'processStartedAt': processStartedAt,
      'bindHost': bindHost,
      'bindPort': bindPort,
      'publicEndpoint': publicEndpoint,
      'buildVersion': buildVersion,
      'status': status,
      'healthState': healthState,
      'lastHeartbeatAt': lastHeartbeatAt,
      'lastOnlineAt': lastOnlineAt,
      'uptimeSeconds': uptimeSeconds,
      'metrics': metrics,
      'joinMode': joinMode,
      'qualityScore': qualityScore,
      'desiredConfigRevision': desiredConfigRevision,
      'appliedConfigRevision': appliedConfigRevision,
      'desiredApplicationsRevision': desiredApplicationsRevision,
      'appliedApplicationsRevision': appliedApplicationsRevision,
      'syncStatus': syncStatus,
      'routingEnabled': routingEnabled,
      'draining': draining,
      'ejected': ejected,
      'restartCount': restartCount,
      'labels': labels?.map((key, item) => MapEntry(key, item)),
      'routingWeight': routingWeight,
      'maintenanceNote': maintenanceNote,
      'probeFailures': probeFailures,
      'probeUrl': probeUrl,
      'createdAt': createdAt,
      'updatedAt': updatedAt,
    };
  }
}

class UpdateClusterInstanceRequest {
  final String? name;
  final int? status;
  final String? publicEndpoint;
  final bool? routingEnabled;
  final bool? draining;
  final String? probeUrl;
  final Map<String, String>? labels;
  final int? routingWeight;
  final String? maintenanceNote;

  UpdateClusterInstanceRequest({
    this.name,
    this.status,
    this.publicEndpoint,
    this.routingEnabled,
    this.draining,
    this.probeUrl,
    this.labels,
    this.routingWeight,
    this.maintenanceNote
  });

  factory UpdateClusterInstanceRequest.fromJson(Map<String, dynamic> json) {
    return UpdateClusterInstanceRequest(
      name: json['name']?.toString(),
      status: json['status'] is int ? json['status'] : null,
      publicEndpoint: json['publicEndpoint']?.toString(),
      routingEnabled: json['routingEnabled'] is bool ? json['routingEnabled'] : null,
      draining: json['draining'] is bool ? json['draining'] : null,
      probeUrl: json['probeUrl']?.toString(),
      labels: (() {
        final map = _sdkworkAsMap(json['labels']);
        if (map == null) {
          return null;
        }
        final result = <String, String>{};
        map.forEach((key, item) {
          final deserialized = item?.toString();
          if (deserialized is String) {
            result[key] = deserialized;
          }
        });
        return result;
      })(),
      routingWeight: json['routingWeight'] is int ? json['routingWeight'] : null,
      maintenanceNote: json['maintenanceNote']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'name': name,
      'status': status,
      'publicEndpoint': publicEndpoint,
      'routingEnabled': routingEnabled,
      'draining': draining,
      'probeUrl': probeUrl,
      'labels': labels?.map((key, item) => MapEntry(key, item)),
      'routingWeight': routingWeight,
      'maintenanceNote': maintenanceNote,
    };
  }
}

class ClusterEventResponse {
  final String? id;
  final String? clusterId;
  final String? hostId;
  final String? instanceId;
  final String? eventType;
  final String? severity;
  final String? message;
  final Map<String, dynamic>? detail;
  final String? occurredAt;
  final String? createdAt;

  ClusterEventResponse({
    this.id,
    this.clusterId,
    this.hostId,
    this.instanceId,
    this.eventType,
    this.severity,
    this.message,
    this.detail,
    this.occurredAt,
    this.createdAt
  });

  factory ClusterEventResponse.fromJson(Map<String, dynamic> json) {
    return ClusterEventResponse(
      id: json['id']?.toString(),
      clusterId: json['clusterId']?.toString(),
      hostId: json['hostId']?.toString(),
      instanceId: json['instanceId']?.toString(),
      eventType: json['eventType']?.toString(),
      severity: json['severity']?.toString(),
      message: json['message']?.toString(),
      detail: _sdkworkAsMap(json['detail']),
      occurredAt: json['occurredAt']?.toString(),
      createdAt: json['createdAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'id': id,
      'clusterId': clusterId,
      'hostId': hostId,
      'instanceId': instanceId,
      'eventType': eventType,
      'severity': severity,
      'message': message,
      'detail': detail,
      'occurredAt': occurredAt,
      'createdAt': createdAt,
    };
  }
}

class ClusterOverviewResponse {
  final String? totalHosts;
  final String? onlineHosts;
  final String? totalInstances;
  final String? onlineInstances;
  final String? unhealthyInstances;
  final String? pendingPeerMessages;
  final String? generatedAt;

  ClusterOverviewResponse({
    this.totalHosts,
    this.onlineHosts,
    this.totalInstances,
    this.onlineInstances,
    this.unhealthyInstances,
    this.pendingPeerMessages,
    this.generatedAt
  });

  factory ClusterOverviewResponse.fromJson(Map<String, dynamic> json) {
    return ClusterOverviewResponse(
      totalHosts: json['totalHosts']?.toString(),
      onlineHosts: json['onlineHosts']?.toString(),
      totalInstances: json['totalInstances']?.toString(),
      onlineInstances: json['onlineInstances']?.toString(),
      unhealthyInstances: json['unhealthyInstances']?.toString(),
      pendingPeerMessages: json['pendingPeerMessages']?.toString(),
      generatedAt: json['generatedAt']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'totalHosts': totalHosts,
      'onlineHosts': onlineHosts,
      'totalInstances': totalInstances,
      'onlineInstances': onlineInstances,
      'unhealthyInstances': unhealthyInstances,
      'pendingPeerMessages': pendingPeerMessages,
      'generatedAt': generatedAt,
    };
  }
}

class TrafficUsageStatisticsResponse {
  final String? dateFrom;
  final String? dateTo;
  final bool? platformScope;
  final List<TrafficUsageTotal>? totals;
  final List<TrafficUsageDailyPoint>? daily;
  final List<TrafficUsageAppTotal>? apps;
  final List<TrafficUsageTenantTotal>? tenants;

  TrafficUsageStatisticsResponse({
    this.dateFrom,
    this.dateTo,
    this.platformScope,
    this.totals,
    this.daily,
    this.apps,
    this.tenants
  });

  factory TrafficUsageStatisticsResponse.fromJson(Map<String, dynamic> json) {
    return TrafficUsageStatisticsResponse(
      dateFrom: json['dateFrom']?.toString(),
      dateTo: json['dateTo']?.toString(),
      platformScope: json['platformScope'] is bool ? json['platformScope'] : null,
      totals: (() {
        final list = _sdkworkAsList(json['totals']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : TrafficUsageTotal.fromJson(map);
      })())
            .whereType<TrafficUsageTotal>()
            .toList();
      })(),
      daily: (() {
        final list = _sdkworkAsList(json['daily']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : TrafficUsageDailyPoint.fromJson(map);
      })())
            .whereType<TrafficUsageDailyPoint>()
            .toList();
      })(),
      apps: (() {
        final list = _sdkworkAsList(json['apps']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : TrafficUsageAppTotal.fromJson(map);
      })())
            .whereType<TrafficUsageAppTotal>()
            .toList();
      })(),
      tenants: (() {
        final list = _sdkworkAsList(json['tenants']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : TrafficUsageTenantTotal.fromJson(map);
      })())
            .whereType<TrafficUsageTenantTotal>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'dateFrom': dateFrom,
      'dateTo': dateTo,
      'platformScope': platformScope,
      'totals': totals?.map((item) => item.toJson()).toList(),
      'daily': daily?.map((item) => item.toJson()).toList(),
      'apps': apps?.map((item) => item.toJson()).toList(),
      'tenants': tenants?.map((item) => item.toJson()).toList(),
    };
  }
}

class TrafficUsageTotal {
  final String? dimension;
  final String? quantity;
  final String? unit;

  TrafficUsageTotal({
    this.dimension,
    this.quantity,
    this.unit
  });

  factory TrafficUsageTotal.fromJson(Map<String, dynamic> json) {
    return TrafficUsageTotal(
      dimension: json['dimension']?.toString(),
      quantity: json['quantity']?.toString(),
      unit: json['unit']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'dimension': dimension,
      'quantity': quantity,
      'unit': unit,
    };
  }
}

class TrafficUsageDailyPoint {
  final String? usageDate;
  final String? dimension;
  final String? quantity;

  TrafficUsageDailyPoint({
    this.usageDate,
    this.dimension,
    this.quantity
  });

  factory TrafficUsageDailyPoint.fromJson(Map<String, dynamic> json) {
    return TrafficUsageDailyPoint(
      usageDate: json['usageDate']?.toString(),
      dimension: json['dimension']?.toString(),
      quantity: json['quantity']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'usageDate': usageDate,
      'dimension': dimension,
      'quantity': quantity,
    };
  }
}

class TrafficUsageAppTotal {
  final String? appUuid;
  final String? appSlug;
  final String? dimension;
  final String? quantity;
  final String? unit;

  TrafficUsageAppTotal({
    this.appUuid,
    this.appSlug,
    this.dimension,
    this.quantity,
    this.unit
  });

  factory TrafficUsageAppTotal.fromJson(Map<String, dynamic> json) {
    return TrafficUsageAppTotal(
      appUuid: json['appUuid']?.toString(),
      appSlug: json['appSlug']?.toString(),
      dimension: json['dimension']?.toString(),
      quantity: json['quantity']?.toString(),
      unit: json['unit']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'appUuid': appUuid,
      'appSlug': appSlug,
      'dimension': dimension,
      'quantity': quantity,
      'unit': unit,
    };
  }
}

class TrafficUsageTenantTotal {
  final String? tenantId;
  final String? dimension;
  final String? quantity;
  final String? unit;

  TrafficUsageTenantTotal({
    this.tenantId,
    this.dimension,
    this.quantity,
    this.unit
  });

  factory TrafficUsageTenantTotal.fromJson(Map<String, dynamic> json) {
    return TrafficUsageTenantTotal(
      tenantId: json['tenantId']?.toString(),
      dimension: json['dimension']?.toString(),
      quantity: json['quantity']?.toString(),
      unit: json['unit']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'tenantId': tenantId,
      'dimension': dimension,
      'quantity': quantity,
      'unit': unit,
    };
  }
}

class MetricsSummaryResponse {
  final String? asOf;
  final bool? platformScope;
  final String? trafficSince;
  final List<MetricsWindowBounds>? windows;
  final List<MetricsMetricTotals>? entities;
  final List<MetricsMetricTotals>? traffic;
  final List<MetricsMetricTotals>? storage;
  final List<MetricsSeries>? series;
  final MetricsSeriesWindow? seriesWindow;
  final List<String>? unassembledMetrics;

  MetricsSummaryResponse({
    this.asOf,
    this.platformScope,
    this.trafficSince,
    this.windows,
    this.entities,
    this.traffic,
    this.storage,
    this.series,
    this.seriesWindow,
    this.unassembledMetrics
  });

  factory MetricsSummaryResponse.fromJson(Map<String, dynamic> json) {
    return MetricsSummaryResponse(
      asOf: json['asOf']?.toString(),
      platformScope: json['platformScope'] is bool ? json['platformScope'] : null,
      trafficSince: json['trafficSince']?.toString(),
      windows: (() {
        final list = _sdkworkAsList(json['windows']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MetricsWindowBounds.fromJson(map);
      })())
            .whereType<MetricsWindowBounds>()
            .toList();
      })(),
      entities: (() {
        final list = _sdkworkAsList(json['entities']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MetricsMetricTotals.fromJson(map);
      })())
            .whereType<MetricsMetricTotals>()
            .toList();
      })(),
      traffic: (() {
        final list = _sdkworkAsList(json['traffic']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MetricsMetricTotals.fromJson(map);
      })())
            .whereType<MetricsMetricTotals>()
            .toList();
      })(),
      storage: (() {
        final list = _sdkworkAsList(json['storage']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MetricsMetricTotals.fromJson(map);
      })())
            .whereType<MetricsMetricTotals>()
            .toList();
      })(),
      series: (() {
        final list = _sdkworkAsList(json['series']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MetricsSeries.fromJson(map);
      })())
            .whereType<MetricsSeries>()
            .toList();
      })(),
      seriesWindow: (() {
        final map = _sdkworkAsMap(json['seriesWindow']);
        return map == null ? null : MetricsSeriesWindow.fromJson(map);
      })(),
      unassembledMetrics: (() {
        final list = _sdkworkAsList(json['unassembledMetrics']);
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
      'asOf': asOf,
      'platformScope': platformScope,
      'trafficSince': trafficSince,
      'windows': windows?.map((item) => item.toJson()).toList(),
      'entities': entities?.map((item) => item.toJson()).toList(),
      'traffic': traffic?.map((item) => item.toJson()).toList(),
      'storage': storage?.map((item) => item.toJson()).toList(),
      'series': series?.map((item) => item.toJson()).toList(),
      'seriesWindow': seriesWindow?.toJson(),
      'unassembledMetrics': unassembledMetrics?.map((item) => item).toList(),
    };
  }
}

class MetricsWindowBounds {
  final String? window;
  final String? dateFrom;
  final String? dateTo;

  MetricsWindowBounds({
    this.window,
    this.dateFrom,
    this.dateTo
  });

  factory MetricsWindowBounds.fromJson(Map<String, dynamic> json) {
    return MetricsWindowBounds(
      window: json['window']?.toString(),
      dateFrom: json['dateFrom']?.toString(),
      dateTo: json['dateTo']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'window': window,
      'dateFrom': dateFrom,
      'dateTo': dateTo,
    };
  }
}

class MetricsMetricTotals {
  final String? metric;
  final String? unit;
  final List<MetricsWindowValue>? values;

  MetricsMetricTotals({
    this.metric,
    this.unit,
    this.values
  });

  factory MetricsMetricTotals.fromJson(Map<String, dynamic> json) {
    return MetricsMetricTotals(
      metric: json['metric']?.toString(),
      unit: json['unit']?.toString(),
      values: (() {
        final list = _sdkworkAsList(json['values']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MetricsWindowValue.fromJson(map);
      })())
            .whereType<MetricsWindowValue>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'metric': metric,
      'unit': unit,
      'values': values?.map((item) => item.toJson()).toList(),
    };
  }
}

class MetricsWindowValue {
  final String? window;
  final String? quantity;
  final String? unit;

  MetricsWindowValue({
    this.window,
    this.quantity,
    this.unit
  });

  factory MetricsWindowValue.fromJson(Map<String, dynamic> json) {
    return MetricsWindowValue(
      window: json['window']?.toString(),
      quantity: json['quantity']?.toString(),
      unit: json['unit']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'window': window,
      'quantity': quantity,
      'unit': unit,
    };
  }
}

class MetricsSeriesWindow {
  final String? dateFrom;
  final String? dateTo;

  MetricsSeriesWindow({
    this.dateFrom,
    this.dateTo
  });

  factory MetricsSeriesWindow.fromJson(Map<String, dynamic> json) {
    return MetricsSeriesWindow(
      dateFrom: json['dateFrom']?.toString(),
      dateTo: json['dateTo']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'dateFrom': dateFrom,
      'dateTo': dateTo,
    };
  }
}

class MetricsSeries {
  final String? metric;
  final String? unit;
  final List<MetricsSeriesPoint>? points;

  MetricsSeries({
    this.metric,
    this.unit,
    this.points
  });

  factory MetricsSeries.fromJson(Map<String, dynamic> json) {
    return MetricsSeries(
      metric: json['metric']?.toString(),
      unit: json['unit']?.toString(),
      points: (() {
        final list = _sdkworkAsList(json['points']);
        if (list == null) {
          return null;
        }
        return list
            .map((item) => (() {
        final map = _sdkworkAsMap(item);
        return map == null ? null : MetricsSeriesPoint.fromJson(map);
      })())
            .whereType<MetricsSeriesPoint>()
            .toList();
      })()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'metric': metric,
      'unit': unit,
      'points': points?.map((item) => item.toJson()).toList(),
    };
  }
}

class MetricsSeriesPoint {
  final String? date;
  final String? quantity;

  MetricsSeriesPoint({
    this.date,
    this.quantity
  });

  factory MetricsSeriesPoint.fromJson(Map<String, dynamic> json) {
    return MetricsSeriesPoint(
      date: json['date']?.toString(),
      quantity: json['quantity']?.toString()
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'date': date,
      'quantity': quantity,
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

class RootDomainsUpdateResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  RootDomainsUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory RootDomainsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return RootDomainsUpdateResponse(
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

class WebserverConfigsListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  WebserverConfigsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory WebserverConfigsListResponse.fromJson(Map<String, dynamic> json) {
    return WebserverConfigsListResponse(
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

class WebserverConfigsRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  WebserverConfigsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory WebserverConfigsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return WebserverConfigsRetrieveResponse(
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

class WebserverConfigsUpdateResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  WebserverConfigsUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory WebserverConfigsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return WebserverConfigsUpdateResponse(
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

class ClustersListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersListResponse.fromJson(Map<String, dynamic> json) {
    return ClustersListResponse(
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

class ClustersCreateResponse201 {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ClustersCreateResponse201(
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

class ClustersRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ClustersRetrieveResponse(
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

class ClustersUpdateResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ClustersUpdateResponse(
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

class ClustersSyncResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersSyncResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersSyncResponse.fromJson(Map<String, dynamic> json) {
    return ClustersSyncResponse(
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

class ClustersHostsListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersHostsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersHostsListResponse.fromJson(Map<String, dynamic> json) {
    return ClustersHostsListResponse(
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

class ClustersHostsRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersHostsRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersHostsRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ClustersHostsRetrieveResponse(
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

class ClustersHostsUpdateResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersHostsUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersHostsUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ClustersHostsUpdateResponse(
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

class ClustersInstancesListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesListResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesListResponse(
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

class ClustersInstancesRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesRetrieveResponse(
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

class ClustersInstancesUpdateResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesUpdateResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesUpdateResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesUpdateResponse(
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

class ClustersEventsListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersEventsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersEventsListResponse.fromJson(Map<String, dynamic> json) {
    return ClustersEventsListResponse(
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

class ClustersOverviewRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersOverviewRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersOverviewRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return ClustersOverviewRetrieveResponse(
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

class ClustersInstancesHeartbeatsListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesHeartbeatsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesHeartbeatsListResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesHeartbeatsListResponse(
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

class ClustersInstancesMetricsListResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesMetricsListResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesMetricsListResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesMetricsListResponse(
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

class ClustersInstancesProbeResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesProbeResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesProbeResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesProbeResponse(
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

class ClustersInstancesDrainResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesDrainResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesDrainResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesDrainResponse(
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

class ClustersInstancesUndrainResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesUndrainResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesUndrainResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesUndrainResponse(
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

class ClustersInstancesCordonResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesCordonResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesCordonResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesCordonResponse(
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

class ClustersInstancesUncordonResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersInstancesUncordonResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersInstancesUncordonResponse.fromJson(Map<String, dynamic> json) {
    return ClustersInstancesUncordonResponse(
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

class ClustersMessagesCreateResponse201 {
  final int? code;
  final dynamic data;
  final String? traceId;

  ClustersMessagesCreateResponse201({
    this.code,
    this.data,
    this.traceId
  });

  factory ClustersMessagesCreateResponse201.fromJson(Map<String, dynamic> json) {
    return ClustersMessagesCreateResponse201(
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

class TrafficUsagesRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  TrafficUsagesRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory TrafficUsagesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return TrafficUsagesRetrieveResponse(
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

class PlatformTrafficUsagesRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  PlatformTrafficUsagesRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory PlatformTrafficUsagesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return PlatformTrafficUsagesRetrieveResponse(
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

class MetricsSummariesRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  MetricsSummariesRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory MetricsSummariesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return MetricsSummariesRetrieveResponse(
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

class PlatformMetricsSummariesRetrieveResponse {
  final int? code;
  final dynamic data;
  final String? traceId;

  PlatformMetricsSummariesRetrieveResponse({
    this.code,
    this.data,
    this.traceId
  });

  factory PlatformMetricsSummariesRetrieveResponse.fromJson(Map<String, dynamic> json) {
    return PlatformMetricsSummariesRetrieveResponse(
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
