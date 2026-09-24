import Foundation

public struct ClusterHeartbeatSampleResponse: Codable {
    public let id: String?
    public let status: Int?
    public let latencyMs: Int?
    public let metrics: [String: Any]?
    public let reportedAt: String?


    public init(id: String? = nil, status: Int? = nil, latencyMs: Int? = nil, metrics: [String: Any]? = nil, reportedAt: String? = nil) {
        self.id = id
        self.status = status
        self.latencyMs = latencyMs
        self.metrics = metrics
        self.reportedAt = reportedAt
    }
}

public struct PublishClusterSyncRequest: Codable {
    public let kind: String?
    public let payload: [String: Any]?


    public init(kind: String? = nil, payload: [String: Any]? = nil) {
        self.kind = kind
        self.payload = payload
    }
}

public struct ClusterSyncManifest: Codable {
    public let clusterId: String?
    public let kind: String?
    public let revision: String?
    public let sha256: String?
    public let payload: [String: Any]?
    public let createdAt: String?


    public init(clusterId: String? = nil, kind: String? = nil, revision: String? = nil, sha256: String? = nil, payload: [String: Any]? = nil, createdAt: String? = nil) {
        self.clusterId = clusterId
        self.kind = kind
        self.revision = revision
        self.sha256 = sha256
        self.payload = payload
        self.createdAt = createdAt
    }
}

public struct ProbeClusterInstanceRequest: Codable {
    public let path: String?
    public let timeoutMs: Int?


    public init(path: String? = nil, timeoutMs: Int? = nil) {
        self.path = path
        self.timeoutMs = timeoutMs
    }
}

public struct ClusterProbeRunResponse: Codable {
    public let healthy: Bool?
    public let latencyMs: Int?
    public let failures: Int?
    public let ejected: Bool?
    public let recovered: Bool?


    public init(healthy: Bool? = nil, latencyMs: Int? = nil, failures: Int? = nil, ejected: Bool? = nil, recovered: Bool? = nil) {
        self.healthy = healthy
        self.latencyMs = latencyMs
        self.failures = failures
        self.ejected = ejected
        self.recovered = recovered
    }
}

public struct EnqueueClusterPeerMessagesRequest: Codable {
    public let clusterId: String?
    public let toInstanceId: String?
    public let fromInstanceId: String?
    public let messageType: String?
    public let payload: [String: Any]?
    public let expiresInSeconds: Int?


    public init(clusterId: String? = nil, toInstanceId: String? = nil, fromInstanceId: String? = nil, messageType: String? = nil, payload: [String: Any]? = nil, expiresInSeconds: Int? = nil) {
        self.clusterId = clusterId
        self.toInstanceId = toInstanceId
        self.fromInstanceId = fromInstanceId
        self.messageType = messageType
        self.payload = payload
        self.expiresInSeconds = expiresInSeconds
    }
}

public struct EnqueueClusterPeerMessagesResponse: Codable {
    public let enqueued: String?


    public init(enqueued: String? = nil) {
        self.enqueued = enqueued
    }
}

public struct ProblemDetail: Codable {
    public let type: String?
    public let title: String?
    public let status: Int?
    public let detail: String?
    public let instance: String?
    public let code: Int?
    public let traceId: String?
    public let errors: [FieldError]?


    public init(type: String? = nil, title: String? = nil, status: Int? = nil, detail: String? = nil, instance: String? = nil, code: Int? = nil, traceId: String? = nil, errors: [FieldError]? = nil) {
        self.type = type
        self.title = title
        self.status = status
        self.detail = detail
        self.instance = instance
        self.code = code
        self.traceId = traceId
        self.errors = errors
    }
}

public struct CreateNginxConfigRequest: Codable {
    public let configType: Int?
    public let configName: String?
    public let configContent: String?
    public let siteId: String?


    public init(configType: Int? = nil, configName: String? = nil, configContent: String? = nil, siteId: String? = nil) {
        self.configType = configType
        self.configName = configName
        self.configContent = configContent
        self.siteId = siteId
    }
}

public struct UpdateNginxConfigRequest: Codable {
    public let configContent: String?
    public let configName: String?


    public init(configContent: String? = nil, configName: String? = nil) {
        self.configContent = configContent
        self.configName = configName
    }
}

public struct MediaChecksum: Codable {
    public let algorithm: String?
    public let value: String?


    public init(algorithm: String? = nil, value: String? = nil) {
        self.algorithm = algorithm
        self.value = value
    }
}

public struct MediaResource: Codable {
    public let id: String?
    public let kind: String?
    public let source: String?
    public let url: String?
    public let publicUrl: String?
    public let uri: String?
    public let objectBlobId: String?
    public let fileName: String?
    public let mimeType: String?
    public let sizeBytes: String?
    public let checksum: MediaChecksum?
    public let width: Int?
    public let height: Int?
    public let durationSeconds: Double?
    public let altText: String?
    public let title: String?
    public let metadata: [String: Any]?


    public init(id: String? = nil, kind: String? = nil, source: String? = nil, url: String? = nil, publicUrl: String? = nil, uri: String? = nil, objectBlobId: String? = nil, fileName: String? = nil, mimeType: String? = nil, sizeBytes: String? = nil, checksum: MediaChecksum? = nil, width: Int? = nil, height: Int? = nil, durationSeconds: Double? = nil, altText: String? = nil, title: String? = nil, metadata: [String: Any]? = nil) {
        self.id = id
        self.kind = kind
        self.source = source
        self.url = url
        self.publicUrl = publicUrl
        self.uri = uri
        self.objectBlobId = objectBlobId
        self.fileName = fileName
        self.mimeType = mimeType
        self.sizeBytes = sizeBytes
        self.checksum = checksum
        self.width = width
        self.height = height
        self.durationSeconds = durationSeconds
        self.altText = altText
        self.title = title
        self.metadata = metadata
    }
}

public struct PlatformTargetResponse: Codable {
    public let id: String?
    public let appId: String?
    public let targetKey: String?
    public let platform: String?
    public let techStack: String?
    public let architectures: [String]?
    public let bundleId: String?
    public let packageName: String?
    public let appIdValue: String?
    public let bundleName: String?
    public let targetStatus: String?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, appId: String? = nil, targetKey: String? = nil, platform: String? = nil, techStack: String? = nil, architectures: [String]? = nil, bundleId: String? = nil, packageName: String? = nil, appIdValue: String? = nil, bundleName: String? = nil, targetStatus: String? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.appId = appId
        self.targetKey = targetKey
        self.platform = platform
        self.techStack = techStack
        self.architectures = architectures
        self.bundleId = bundleId
        self.packageName = packageName
        self.appIdValue = appIdValue
        self.bundleName = bundleName
        self.targetStatus = targetStatus
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct CreatePlatformTargetRequest: Codable {
    public let targetKey: String?
    public let platform: String?
    public let techStack: String?
    public let architectures: [String]?
    public let bundleId: String?
    public let packageName: String?
    public let appId: String?
    public let bundleName: String?
    public let allowedChannels: [String]?


    public init(targetKey: String? = nil, platform: String? = nil, techStack: String? = nil, architectures: [String]? = nil, bundleId: String? = nil, packageName: String? = nil, appId: String? = nil, bundleName: String? = nil, allowedChannels: [String]? = nil) {
        self.targetKey = targetKey
        self.platform = platform
        self.techStack = techStack
        self.architectures = architectures
        self.bundleId = bundleId
        self.packageName = packageName
        self.appId = appId
        self.bundleName = bundleName
        self.allowedChannels = allowedChannels
    }
}

public struct ApplicationStoreListing: Codable {
    public let icon: MediaResource?
    public let cover: MediaResource?
    public let previews: [MediaResource]?
    public let shortDescription: String?
    public let fullDescription: String?
    public let releaseNotes: String?
    public let category: String?
    public let keywords: [String]?
    public let supportUrl: String?
    public let privacyPolicyUrl: String?
    public let officialWebsiteUrl: String?


    public init(icon: MediaResource? = nil, cover: MediaResource? = nil, previews: [MediaResource]? = nil, shortDescription: String? = nil, fullDescription: String? = nil, releaseNotes: String? = nil, category: String? = nil, keywords: [String]? = nil, supportUrl: String? = nil, privacyPolicyUrl: String? = nil, officialWebsiteUrl: String? = nil) {
        self.icon = icon
        self.cover = cover
        self.previews = previews
        self.shortDescription = shortDescription
        self.fullDescription = fullDescription
        self.releaseNotes = releaseNotes
        self.category = category
        self.keywords = keywords
        self.supportUrl = supportUrl
        self.privacyPolicyUrl = privacyPolicyUrl
        self.officialWebsiteUrl = officialWebsiteUrl
    }
}

public struct CreateApplicationRequest: Codable {
    public let name: String?
    public let slug: String?
    public let description: String?
    public let appKind: String?
    public let runtimeConfig: [String: Any]?
    public let storeListing: ApplicationStoreListing?


    public init(name: String? = nil, slug: String? = nil, description: String? = nil, appKind: String? = nil, runtimeConfig: [String: Any]? = nil, storeListing: ApplicationStoreListing? = nil) {
        self.name = name
        self.slug = slug
        self.description = description
        self.appKind = appKind
        self.runtimeConfig = runtimeConfig
        self.storeListing = storeListing
    }
}

public struct UpdateApplicationRequest: Codable {
    public let name: String?
    public let description: String?
    public let runtimeConfig: [String: Any]?
    public let storeListing: ApplicationStoreListing?


    public init(name: String? = nil, description: String? = nil, runtimeConfig: [String: Any]? = nil, storeListing: ApplicationStoreListing? = nil) {
        self.name = name
        self.description = description
        self.runtimeConfig = runtimeConfig
        self.storeListing = storeListing
    }
}

public struct ApplicationResponse: Codable {
    public let id: String?
    public let name: String?
    public let slug: String?
    public let description: String?
    public let appKind: String?
    public let siteType: Int?
    public let status: Int?
    public let hasSourceVersion: Bool?
    public let runtimeConfig: [String: Any]?
    public let storeListing: ApplicationStoreListing?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, name: String? = nil, slug: String? = nil, description: String? = nil, appKind: String? = nil, siteType: Int? = nil, status: Int? = nil, hasSourceVersion: Bool? = nil, runtimeConfig: [String: Any]? = nil, storeListing: ApplicationStoreListing? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.name = name
        self.slug = slug
        self.description = description
        self.appKind = appKind
        self.siteType = siteType
        self.status = status
        self.hasSourceVersion = hasSourceVersion
        self.runtimeConfig = runtimeConfig
        self.storeListing = storeListing
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct CreateApplicationDomainRequest: Codable {
    public let hostname: String?
    public let isPrimary: Bool?
    public let sslEnabled: Bool?
    public let sslProvider: String?


    public init(hostname: String? = nil, isPrimary: Bool? = nil, sslEnabled: Bool? = nil, sslProvider: String? = nil) {
        self.hostname = hostname
        self.isPrimary = isPrimary
        self.sslEnabled = sslEnabled
        self.sslProvider = sslProvider
    }
}

public struct CreateManagedDomainRequest: Codable {
    public let hostname: String?
    public let applicationId: String?
    public let isPrimary: Bool?
    public let sslEnabled: Bool?
    public let sslProvider: String?


    public init(hostname: String? = nil, applicationId: String? = nil, isPrimary: Bool? = nil, sslEnabled: Bool? = nil, sslProvider: String? = nil) {
        self.hostname = hostname
        self.applicationId = applicationId
        self.isPrimary = isPrimary
        self.sslEnabled = sslEnabled
        self.sslProvider = sslProvider
    }
}

public struct CreateRootDomainRequest: Codable {
    public let hostname: String?


    public init(hostname: String? = nil) {
        self.hostname = hostname
    }
}

public struct UpdateRootDomainRequest: Codable {
    public let displayName: String?
    public let dnsProvider: String?
    public let providerZoneRef: String?
    public let status: Int?


    public init(displayName: String? = nil, dnsProvider: String? = nil, providerZoneRef: String? = nil, status: Int? = nil) {
        self.displayName = displayName
        self.dnsProvider = dnsProvider
        self.providerZoneRef = providerZoneRef
        self.status = status
    }
}

public struct CreateRootDomainHostnameRequest: Codable {
    public let recordName: String?
    public let applicationId: String?
    public let isPrimary: Bool?
    public let sslEnabled: Bool?
    public let sslProvider: String?


    public init(recordName: String? = nil, applicationId: String? = nil, isPrimary: Bool? = nil, sslEnabled: Bool? = nil, sslProvider: String? = nil) {
        self.recordName = recordName
        self.applicationId = applicationId
        self.isPrimary = isPrimary
        self.sslEnabled = sslEnabled
        self.sslProvider = sslProvider
    }
}

public struct RootDomainResponse: Codable {
    public let id: String?
    public let hostname: String?
    public let displayName: String?
    public let dnsProvider: String?
    public let providerZoneRef: String?
    public let status: Int?
    public let subdomainCount: String?
    public let boundSubdomainCount: String?
    public let verifiedSubdomainCount: String?
    public let httpsSubdomainCount: String?
    public let activeDeploymentCount: String?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, hostname: String? = nil, displayName: String? = nil, dnsProvider: String? = nil, providerZoneRef: String? = nil, status: Int? = nil, subdomainCount: String? = nil, boundSubdomainCount: String? = nil, verifiedSubdomainCount: String? = nil, httpsSubdomainCount: String? = nil, activeDeploymentCount: String? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.hostname = hostname
        self.displayName = displayName
        self.dnsProvider = dnsProvider
        self.providerZoneRef = providerZoneRef
        self.status = status
        self.subdomainCount = subdomainCount
        self.boundSubdomainCount = boundSubdomainCount
        self.verifiedSubdomainCount = verifiedSubdomainCount
        self.httpsSubdomainCount = httpsSubdomainCount
        self.activeDeploymentCount = activeDeploymentCount
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct DomainDeploymentResponse: Codable {
    public let id: String?
    public let status: Int?
    public let environment: String?
    public let versionTag: String?
    public let completedAt: String?
    public let createdAt: String?


    public init(id: String? = nil, status: Int? = nil, environment: String? = nil, versionTag: String? = nil, completedAt: String? = nil, createdAt: String? = nil) {
        self.id = id
        self.status = status
        self.environment = environment
        self.versionTag = versionTag
        self.completedAt = completedAt
        self.createdAt = createdAt
    }
}

public struct UpdateDomainApplicationBindingRequest: Codable {
    public let applicationId: String?
    public let isPrimary: Bool?


    public init(applicationId: String? = nil, isPrimary: Bool? = nil) {
        self.applicationId = applicationId
        self.isPrimary = isPrimary
    }
}

public struct ApplicationDomainResponse: Codable {
    public let id: String?
    public let hostname: String?
    public let rootDomainId: String?
    public let recordName: String?
    public let applicationId: String?
    public let applicationName: String?
    public let certificateCount: String?
    public let isPrimary: Bool?
    public let isVerified: Bool?
    public let sslEnabled: Bool?
    public let sslProvider: String?
    public let status: Int?
    public let latestDeployment: DomainDeploymentResponse?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, hostname: String? = nil, rootDomainId: String? = nil, recordName: String? = nil, applicationId: String? = nil, applicationName: String? = nil, certificateCount: String? = nil, isPrimary: Bool? = nil, isVerified: Bool? = nil, sslEnabled: Bool? = nil, sslProvider: String? = nil, status: Int? = nil, latestDeployment: DomainDeploymentResponse? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.hostname = hostname
        self.rootDomainId = rootDomainId
        self.recordName = recordName
        self.applicationId = applicationId
        self.applicationName = applicationName
        self.certificateCount = certificateCount
        self.isPrimary = isPrimary
        self.isVerified = isVerified
        self.sslEnabled = sslEnabled
        self.sslProvider = sslProvider
        self.status = status
        self.latestDeployment = latestDeployment
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct DomainVerifyResponse: Codable {
    public let verified: Bool?
    public let status: String?
    public let method: String?
    public let recordName: String?
    public let recordValue: String?
    public let attemptCount: Int?
    public let expiresAt: String?
    public let nextAttemptAt: String?
    public let checkedAt: String?
    public let failureCode: String?


    public init(verified: Bool? = nil, status: String? = nil, method: String? = nil, recordName: String? = nil, recordValue: String? = nil, attemptCount: Int? = nil, expiresAt: String? = nil, nextAttemptAt: String? = nil, checkedAt: String? = nil, failureCode: String? = nil) {
        self.verified = verified
        self.status = status
        self.method = method
        self.recordName = recordName
        self.recordValue = recordValue
        self.attemptCount = attemptCount
        self.expiresAt = expiresAt
        self.nextAttemptAt = nextAttemptAt
        self.checkedAt = checkedAt
        self.failureCode = failureCode
    }
}

public struct ApplicationSourceVersionConfigSnapshot: Codable {
    public let appConfigPath: String?
    public let deploymentConfigPath: String?
    public let appConfigDetected: Bool?
    public let deploymentConfigDetected: Bool?


    public init(appConfigPath: String? = nil, deploymentConfigPath: String? = nil, appConfigDetected: Bool? = nil, deploymentConfigDetected: Bool? = nil) {
        self.appConfigPath = appConfigPath
        self.deploymentConfigPath = deploymentConfigPath
        self.appConfigDetected = appConfigDetected
        self.deploymentConfigDetected = deploymentConfigDetected
    }
}

public struct CreateApplicationSourceVersionRequest: Codable {
    public let versionTag: String?
    public let sourceType: String?
    public let sourceRef: String?
    public let commitHash: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?
    public let configSnapshot: ApplicationSourceVersionConfigSnapshot?


    public init(versionTag: String? = nil, sourceType: String? = nil, sourceRef: String? = nil, commitHash: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil, configSnapshot: ApplicationSourceVersionConfigSnapshot? = nil) {
        self.versionTag = versionTag
        self.sourceType = sourceType
        self.sourceRef = sourceRef
        self.commitHash = commitHash
        self.artifactDriveUri = artifactDriveUri
        self.artifactSize = artifactSize
        self.artifactHash = artifactHash
        self.configSnapshot = configSnapshot
    }
}

public struct ImportApplicationGitSourceVersionRequest: Codable {
    public let versionTag: String?
    public let repositoryUrl: String?
    public let gitRef: String?


    public init(versionTag: String? = nil, repositoryUrl: String? = nil, gitRef: String? = nil) {
        self.versionTag = versionTag
        self.repositoryUrl = repositoryUrl
        self.gitRef = gitRef
    }
}

public struct ApplicationSourceVersionResponse: Codable {
    public let id: String?
    public let siteId: String?
    public let versionTag: String?
    public let sourceType: String?
    public let sourceRef: String?
    public let commitHash: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?
    public let configSnapshot: ApplicationSourceVersionConfigSnapshot?
    public let status: Int?
    public let retained: Bool?
    public let createdAt: String?


    public init(id: String? = nil, siteId: String? = nil, versionTag: String? = nil, sourceType: String? = nil, sourceRef: String? = nil, commitHash: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil, configSnapshot: ApplicationSourceVersionConfigSnapshot? = nil, status: Int? = nil, retained: Bool? = nil, createdAt: String? = nil) {
        self.id = id
        self.siteId = siteId
        self.versionTag = versionTag
        self.sourceType = sourceType
        self.sourceRef = sourceRef
        self.commitHash = commitHash
        self.artifactDriveUri = artifactDriveUri
        self.artifactSize = artifactSize
        self.artifactHash = artifactHash
        self.configSnapshot = configSnapshot
        self.status = status
        self.retained = retained
        self.createdAt = createdAt
    }
}

public struct CreateApplicationDeploymentRequest: Codable {
    public let sourceVersionId: String?
    public let deployType: Int?
    public let environment: String?
    public let versionTag: String?
    public let commitHash: String?
    public let sourceRef: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?


    public init(sourceVersionId: String? = nil, deployType: Int? = nil, environment: String? = nil, versionTag: String? = nil, commitHash: String? = nil, sourceRef: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil) {
        self.sourceVersionId = sourceVersionId
        self.deployType = deployType
        self.environment = environment
        self.versionTag = versionTag
        self.commitHash = commitHash
        self.sourceRef = sourceRef
        self.artifactDriveUri = artifactDriveUri
        self.artifactSize = artifactSize
        self.artifactHash = artifactHash
    }
}

public struct ApplicationDeploymentResponse: Codable {
    public let id: String?
    public let siteId: String?
    public let sourceVersionId: String?
    public let status: Int?
    public let deployType: Int?
    public let environment: String?
    public let versionTag: String?
    public let commitHash: String?
    public let sourceRef: String?
    public let rollbackFromDeploymentId: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?
    public let startedAt: String?
    public let completedAt: String?
    public let durationMs: String?
    public let createdAt: String?


    public init(id: String? = nil, siteId: String? = nil, sourceVersionId: String? = nil, status: Int? = nil, deployType: Int? = nil, environment: String? = nil, versionTag: String? = nil, commitHash: String? = nil, sourceRef: String? = nil, rollbackFromDeploymentId: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil, startedAt: String? = nil, completedAt: String? = nil, durationMs: String? = nil, createdAt: String? = nil) {
        self.id = id
        self.siteId = siteId
        self.sourceVersionId = sourceVersionId
        self.status = status
        self.deployType = deployType
        self.environment = environment
        self.versionTag = versionTag
        self.commitHash = commitHash
        self.sourceRef = sourceRef
        self.rollbackFromDeploymentId = rollbackFromDeploymentId
        self.artifactDriveUri = artifactDriveUri
        self.artifactSize = artifactSize
        self.artifactHash = artifactHash
        self.startedAt = startedAt
        self.completedAt = completedAt
        self.durationMs = durationMs
        self.createdAt = createdAt
    }
}

public struct IssueCertificateRequest: Codable {
    public let domainIds: [String]?
    public let certType: Int?
    public let keyAlgorithm: String?
    public let autoRenew: Bool?


    public init(domainIds: [String]? = nil, certType: Int? = nil, keyAlgorithm: String? = nil, autoRenew: Bool? = nil) {
        self.domainIds = domainIds
        self.certType = certType
        self.keyAlgorithm = keyAlgorithm
        self.autoRenew = autoRenew
    }
}

public struct CertificateIdentifierResponse: Codable {
    public let domainId: String?
    public let hostname: String?
    public let identifierType: String?
    public let position: Int?


    public init(domainId: String? = nil, hostname: String? = nil, identifierType: String? = nil, position: Int? = nil) {
        self.domainId = domainId
        self.hostname = hostname
        self.identifierType = identifierType
        self.position = position
    }
}

public struct UpdateCertificateRequest: Codable {
    public let autoRenew: Bool?


    public init(autoRenew: Bool? = nil) {
        self.autoRenew = autoRenew
    }
}

public struct CertificateResponse: Codable {
    public let id: String?
    public let certName: String?
    public let identifiers: [CertificateIdentifierResponse]?
    public let certType: Int?
    public let issuer: String?
    public let fingerprint: String?
    public let keyAlgorithm: String?
    public let notBefore: String?
    public let notAfter: String?
    public let autoRenew: Bool?
    public let renewalStatus: String?
    public let status: String?
    public let createdAt: String?


    public init(id: String? = nil, certName: String? = nil, identifiers: [CertificateIdentifierResponse]? = nil, certType: Int? = nil, issuer: String? = nil, fingerprint: String? = nil, keyAlgorithm: String? = nil, notBefore: String? = nil, notAfter: String? = nil, autoRenew: Bool? = nil, renewalStatus: String? = nil, status: String? = nil, createdAt: String? = nil) {
        self.id = id
        self.certName = certName
        self.identifiers = identifiers
        self.certType = certType
        self.issuer = issuer
        self.fingerprint = fingerprint
        self.keyAlgorithm = keyAlgorithm
        self.notBefore = notBefore
        self.notAfter = notAfter
        self.autoRenew = autoRenew
        self.renewalStatus = renewalStatus
        self.status = status
        self.createdAt = createdAt
    }
}

public struct RevokeCertificateRequest: Codable {
    public let reason: String?


    public init(reason: String? = nil) {
        self.reason = reason
    }
}

public struct CertificateOperationResponse: Codable {
    public let id: String?
    public let certificateId: String?
    public let operationType: String?
    public let status: String?
    public let attemptCount: Int?
    public let maxAttempts: Int?
    public let nextAttemptAt: String?
    public let failureCode: String?
    public let failureDetail: String?
    public let createdAt: String?
    public let updatedAt: String?
    public let completedAt: String?


    public init(id: String? = nil, certificateId: String? = nil, operationType: String? = nil, status: String? = nil, attemptCount: Int? = nil, maxAttempts: Int? = nil, nextAttemptAt: String? = nil, failureCode: String? = nil, failureDetail: String? = nil, createdAt: String? = nil, updatedAt: String? = nil, completedAt: String? = nil) {
        self.id = id
        self.certificateId = certificateId
        self.operationType = operationType
        self.status = status
        self.attemptCount = attemptCount
        self.maxAttempts = maxAttempts
        self.nextAttemptAt = nextAttemptAt
        self.failureCode = failureCode
        self.failureDetail = failureDetail
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.completedAt = completedAt
    }
}

public struct CreateListenerCertificateBindingRequest: Codable {
    public let certificateId: String?
    public let certificateVersionId: String?
    public let priority: Int?
    public let isDefault: Bool?


    public init(certificateId: String? = nil, certificateVersionId: String? = nil, priority: Int? = nil, isDefault: Bool? = nil) {
        self.certificateId = certificateId
        self.certificateVersionId = certificateVersionId
        self.priority = priority
        self.isDefault = isDefault
    }
}

public struct ListenerCertificateBindingResponse: Codable {
    public let id: String?
    public let siteId: String?
    public let domainId: String?
    public let certificateId: String?
    public let desiredCertificateVersionId: String?
    public let currentCertificateVersionId: String?
    public let desiredCertificate: ListenerCertificateSummaryResponse?
    public let currentCertificate: ListenerCertificateSummaryResponse?
    public let keyAlgorithm: String?
    public let priority: Int?
    public let isDefault: Bool?
    public let status: String?
    public let activatedAt: String?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, siteId: String? = nil, domainId: String? = nil, certificateId: String? = nil, desiredCertificateVersionId: String? = nil, currentCertificateVersionId: String? = nil, desiredCertificate: ListenerCertificateSummaryResponse? = nil, currentCertificate: ListenerCertificateSummaryResponse? = nil, keyAlgorithm: String? = nil, priority: Int? = nil, isDefault: Bool? = nil, status: String? = nil, activatedAt: String? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.siteId = siteId
        self.domainId = domainId
        self.certificateId = certificateId
        self.desiredCertificateVersionId = desiredCertificateVersionId
        self.currentCertificateVersionId = currentCertificateVersionId
        self.desiredCertificate = desiredCertificate
        self.currentCertificate = currentCertificate
        self.keyAlgorithm = keyAlgorithm
        self.priority = priority
        self.isDefault = isDefault
        self.status = status
        self.activatedAt = activatedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct ListenerCertificateSummaryResponse: Codable {
    public let certName: String?
    public let identifiers: [CertificateIdentifierResponse]?
    public let issuer: String?
    public let fingerprint: String?
    public let notAfter: String?
    public let status: String?


    public init(certName: String? = nil, identifiers: [CertificateIdentifierResponse]? = nil, issuer: String? = nil, fingerprint: String? = nil, notAfter: String? = nil, status: String? = nil) {
        self.certName = certName
        self.identifiers = identifiers
        self.issuer = issuer
        self.fingerprint = fingerprint
        self.notAfter = notAfter
        self.status = status
    }
}

public struct CertificateDistributionResponse: Codable {
    public let serverId: String?
    public let serverName: String?
    public let host: String?
    public let desiredSyncVersion: String?
    public let appliedSyncVersion: String?
    public let status: String?
    public let lastHeartbeatAt: String?


    public init(serverId: String? = nil, serverName: String? = nil, host: String? = nil, desiredSyncVersion: String? = nil, appliedSyncVersion: String? = nil, status: String? = nil, lastHeartbeatAt: String? = nil) {
        self.serverId = serverId
        self.serverName = serverName
        self.host = host
        self.desiredSyncVersion = desiredSyncVersion
        self.appliedSyncVersion = appliedSyncVersion
        self.status = status
        self.lastHeartbeatAt = lastHeartbeatAt
    }
}

public struct NginxConfigResponse: Codable {
    public let id: String?
    public let configType: Int?
    public let configName: String?
    public let configContent: String?
    public let configHash: String?
    public let isActive: Bool?
    public let versionNo: Int?
    public let deployedAt: String?
    public let status: Int?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, configType: Int? = nil, configName: String? = nil, configContent: String? = nil, configHash: String? = nil, isActive: Bool? = nil, versionNo: Int? = nil, deployedAt: String? = nil, status: Int? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.configType = configType
        self.configName = configName
        self.configContent = configContent
        self.configHash = configHash
        self.isActive = isActive
        self.versionNo = versionNo
        self.deployedAt = deployedAt
        self.status = status
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct NginxConfigPage: Codable {
    public let items: [NginxConfigResponse]?
    public let total: String?


    public init(items: [NginxConfigResponse]? = nil, total: String? = nil) {
        self.items = items
        self.total = total
    }
}

public struct NginxValidateResponse: Codable {
    public let valid: Bool?
    public let errors: [[String: Any]]?


    public init(valid: Bool? = nil, errors: [[String: Any]]? = nil) {
        self.valid = valid
        self.errors = errors
    }
}

public struct NginxDeployResponse: Codable {
    public let success: Bool?
    public let configId: String?
    public let deployedAt: String?
    public let reloadResult: [String: Any]?


    public init(success: Bool? = nil, configId: String? = nil, deployedAt: String? = nil, reloadResult: [String: Any]? = nil) {
        self.success = success
        self.configId = configId
        self.deployedAt = deployedAt
        self.reloadResult = reloadResult
    }
}

public struct NginxReloadResponse: Codable {
    public let success: Bool?
    public let message: String?
    public let timestamp: String?


    public init(success: Bool? = nil, message: String? = nil, timestamp: String? = nil) {
        self.success = success
        self.message = message
        self.timestamp = timestamp
    }
}

public struct NginxStatusResponse: Codable {
    public let running: Bool?
    public let version: String?
    public let pid: Int?
    public let activeConnections: Int?
    public let configPath: String?
    public let uptime: String?


    public init(running: Bool? = nil, version: String? = nil, pid: Int? = nil, activeConnections: Int? = nil, configPath: String? = nil, uptime: String? = nil) {
        self.running = running
        self.version = version
        self.pid = pid
        self.activeConnections = activeConnections
        self.configPath = configPath
        self.uptime = uptime
    }
}

public struct CreateServerRequest: Codable {
    public let name: String?
    public let host: String?
    public let tenantScopeHash: String?
    public let sshPort: Int?


    public init(name: String? = nil, host: String? = nil, tenantScopeHash: String? = nil, sshPort: Int? = nil) {
        self.name = name
        self.host = host
        self.tenantScopeHash = tenantScopeHash
        self.sshPort = sshPort
    }
}

public struct ServerResponse: Codable {
    public let id: String?
    public let name: String?
    public let host: String?
    public let tenantScopeHash: String?
    public let sshPort: Int?
    public let status: Int?
    public let lastHeartbeatAt: String?
    public let createdAt: String?


    public init(id: String? = nil, name: String? = nil, host: String? = nil, tenantScopeHash: String? = nil, sshPort: Int? = nil, status: Int? = nil, lastHeartbeatAt: String? = nil, createdAt: String? = nil) {
        self.id = id
        self.name = name
        self.host = host
        self.tenantScopeHash = tenantScopeHash
        self.sshPort = sshPort
        self.status = status
        self.lastHeartbeatAt = lastHeartbeatAt
        self.createdAt = createdAt
    }
}

public struct CreateServerResponse: Codable {
    public let id: String?
    public let name: String?
    public let host: String?
    public let tenantScopeHash: String?
    public let sshPort: Int?
    public let status: Int?
    public let lastHeartbeatAt: String?
    public let createdAt: String?
    public let agentToken: String?


    public init(id: String? = nil, name: String? = nil, host: String? = nil, tenantScopeHash: String? = nil, sshPort: Int? = nil, status: Int? = nil, lastHeartbeatAt: String? = nil, createdAt: String? = nil, agentToken: String? = nil) {
        self.id = id
        self.name = name
        self.host = host
        self.tenantScopeHash = tenantScopeHash
        self.sshPort = sshPort
        self.status = status
        self.lastHeartbeatAt = lastHeartbeatAt
        self.createdAt = createdAt
        self.agentToken = agentToken
    }
}

public struct ServerFilesNode: Codable {
    public let id: String?
    public let name: String?
    public let host: String?
    public let sshPort: Int?
    public let status: String?
    public let filesystemRoot: String?
    public let region: String?


    public init(id: String? = nil, name: String? = nil, host: String? = nil, sshPort: Int? = nil, status: String? = nil, filesystemRoot: String? = nil, region: String? = nil) {
        self.id = id
        self.name = name
        self.host = host
        self.sshPort = sshPort
        self.status = status
        self.filesystemRoot = filesystemRoot
        self.region = region
    }
}

public struct ServerDirectoryListing: Codable {
    public let nodeId: String?
    public let path: String?
    public let parentPath: String?
    public let entries: [ServerEntry]?


    public init(nodeId: String? = nil, path: String? = nil, parentPath: String? = nil, entries: [ServerEntry]? = nil) {
        self.nodeId = nodeId
        self.path = path
        self.parentPath = parentPath
        self.entries = entries
    }
}

public struct ServerEntry: Codable {
    public let name: String?
    public let kind: String?
    public let path: String?
    public let size: String?
    public let projectType: String?
    public let isProjectRoot: Bool?


    public init(name: String? = nil, kind: String? = nil, path: String? = nil, size: String? = nil, projectType: String? = nil, isProjectRoot: Bool? = nil) {
        self.name = name
        self.kind = kind
        self.path = path
        self.size = size
        self.projectType = projectType
        self.isProjectRoot = isProjectRoot
    }
}

public struct ServerFileContent: Codable {
    public let nodeId: String?
    public let path: String?
    public let content: String?
    public let size: String?


    public init(nodeId: String? = nil, path: String? = nil, content: String? = nil, size: String? = nil) {
        self.nodeId = nodeId
        self.path = path
        self.content = content
        self.size = size
    }
}

public struct WebserverConfigCatalog: Codable {
    public let configRoot: String?
    public let items: [WebserverConfigEntry]?


    public init(configRoot: String? = nil, items: [WebserverConfigEntry]? = nil) {
        self.configRoot = configRoot
        self.items = items
    }
}

public struct WebserverConfigEntry: Codable {
    public let id: String?
    public let kind: String?
    public let name: String?
    public let path: String?
    public let language: String?
    public let size: String?
    public let updatedAt: String?
    public let writable: Bool?


    public init(id: String? = nil, kind: String? = nil, name: String? = nil, path: String? = nil, language: String? = nil, size: String? = nil, updatedAt: String? = nil, writable: Bool? = nil) {
        self.id = id
        self.kind = kind
        self.name = name
        self.path = path
        self.language = language
        self.size = size
        self.updatedAt = updatedAt
        self.writable = writable
    }
}

public struct WebserverConfigFile: Codable {
    public let id: String?
    public let kind: String?
    public let name: String?
    public let path: String?
    public let language: String?
    public let writable: Bool?
    public let content: String?
    public let size: String?
    public let sha256: String?
    public let updatedAt: String?


    public init(id: String? = nil, kind: String? = nil, name: String? = nil, path: String? = nil, language: String? = nil, writable: Bool? = nil, content: String? = nil, size: String? = nil, sha256: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.kind = kind
        self.name = name
        self.path = path
        self.language = language
        self.writable = writable
        self.content = content
        self.size = size
        self.sha256 = sha256
        self.updatedAt = updatedAt
    }
}

public struct WebserverConfigWriteRequest: Codable {
    public let content: String?
    public let expectedSha256: String?


    public init(content: String? = nil, expectedSha256: String? = nil) {
        self.content = content
        self.expectedSha256 = expectedSha256
    }
}

public struct WebserverConfigWriteResult: Codable {
    public let id: String?
    public let path: String?
    public let size: String?
    public let sha256: String?
    public let backupPath: String?
    public let updatedAt: String?


    public init(id: String? = nil, path: String? = nil, size: String? = nil, sha256: String? = nil, backupPath: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.path = path
        self.size = size
        self.sha256 = sha256
        self.backupPath = backupPath
        self.updatedAt = updatedAt
    }
}

public struct ServerProjectOperations: Codable {
    public let nodeId: String?
    public let path: String?
    public let projectType: String?
    public let operations: [ServerProjectOperation]?


    public init(nodeId: String? = nil, path: String? = nil, projectType: String? = nil, operations: [ServerProjectOperation]? = nil) {
        self.nodeId = nodeId
        self.path = path
        self.projectType = projectType
        self.operations = operations
    }
}

public struct ServerProjectOperation: Codable {
    public let id: String?
    public let kind: String?
    public let label: String?
    public let permission: String?
    public let description: String?
    public let dangerous: Bool?


    public init(id: String? = nil, kind: String? = nil, label: String? = nil, permission: String? = nil, description: String? = nil, dangerous: Bool? = nil) {
        self.id = id
        self.kind = kind
        self.label = label
        self.permission = permission
        self.description = description
        self.dangerous = dangerous
    }
}

public struct ServerRunOperationRequest: Codable {
    public let path: String?
    public let operationId: String?


    public init(path: String? = nil, operationId: String? = nil) {
        self.path = path
        self.operationId = operationId
    }
}

public struct ServerOperationResult: Codable {
    public let operationId: String?
    public let exitCode: Int?
    public let timedOut: Bool?
    public let stdout: String?
    public let stderr: String?
    public let stdoutTruncated: Bool?
    public let stderrTruncated: Bool?
    public let pid: Int?
    public let pidFile: String?
    public let logFile: String?
    public let stopped: Bool?
    public let message: String?


    public init(operationId: String? = nil, exitCode: Int? = nil, timedOut: Bool? = nil, stdout: String? = nil, stderr: String? = nil, stdoutTruncated: Bool? = nil, stderrTruncated: Bool? = nil, pid: Int? = nil, pidFile: String? = nil, logFile: String? = nil, stopped: Bool? = nil, message: String? = nil) {
        self.operationId = operationId
        self.exitCode = exitCode
        self.timedOut = timedOut
        self.stdout = stdout
        self.stderr = stderr
        self.stdoutTruncated = stdoutTruncated
        self.stderrTruncated = stderrTruncated
        self.pid = pid
        self.pidFile = pidFile
        self.logFile = logFile
        self.stopped = stopped
        self.message = message
    }
}

public struct AgentHeartbeatRequest: Codable {
    public let agentVersion: String?
    public let nginxEnabled: Bool?
    public let activeConfigs: String?
    public let lastSyncVersion: String?
    public let certificateObservations: [AgentCertificateObservation]?


    public init(agentVersion: String? = nil, nginxEnabled: Bool? = nil, activeConfigs: String? = nil, lastSyncVersion: String? = nil, certificateObservations: [AgentCertificateObservation]? = nil) {
        self.agentVersion = agentVersion
        self.nginxEnabled = nginxEnabled
        self.activeConfigs = activeConfigs
        self.lastSyncVersion = lastSyncVersion
        self.certificateObservations = certificateObservations
    }
}

public struct AgentCertificateObservation: Codable {
    public let certificateId: String?
    public let fingerprint: String?
    public let syncVersion: String?
    public let state: String?
    public let observedAt: String?
    public let failureCode: String?


    public init(certificateId: String? = nil, fingerprint: String? = nil, syncVersion: String? = nil, state: String? = nil, observedAt: String? = nil, failureCode: String? = nil) {
        self.certificateId = certificateId
        self.fingerprint = fingerprint
        self.syncVersion = syncVersion
        self.state = state
        self.observedAt = observedAt
        self.failureCode = failureCode
    }
}

public struct AgentHeartbeatResponse: Codable {
    public let serverId: String?
    public let status: Int?
    public let acknowledgedAt: String?


    public init(serverId: String? = nil, status: Int? = nil, acknowledgedAt: String? = nil) {
        self.serverId = serverId
        self.status = status
        self.acknowledgedAt = acknowledgedAt
    }
}

public struct AgentSyncResponse: Codable {
    public let serverId: String?
    public let syncVersion: String?
    public let unchanged: Bool?
    public let nginxConfigs: [AgentNginxConfigBundle]?
    public let certificates: [AgentCertificateBundle]?


    public init(serverId: String? = nil, syncVersion: String? = nil, unchanged: Bool? = nil, nginxConfigs: [AgentNginxConfigBundle]? = nil, certificates: [AgentCertificateBundle]? = nil) {
        self.serverId = serverId
        self.syncVersion = syncVersion
        self.unchanged = unchanged
        self.nginxConfigs = nginxConfigs
        self.certificates = certificates
    }
}

public struct AgentNginxConfigBundle: Codable {
    public let configId: String?
    public let domain: String?
    public let configContent: String?
    public let fingerprint: String?
    public let version: String?


    public init(configId: String? = nil, domain: String? = nil, configContent: String? = nil, fingerprint: String? = nil, version: String? = nil) {
        self.configId = configId
        self.domain = domain
        self.configContent = configContent
        self.fingerprint = fingerprint
        self.version = version
    }
}

public struct AgentCertificateBundle: Codable {
    public let certificateId: String?
    public let certName: String?
    public let fingerprint: String?
    public let hostnames: [String]?
    public let fullchainPem: String?
    public let privkeyPem: String?


    public init(certificateId: String? = nil, certName: String? = nil, fingerprint: String? = nil, hostnames: [String]? = nil, fullchainPem: String? = nil, privkeyPem: String? = nil) {
        self.certificateId = certificateId
        self.certName = certName
        self.fingerprint = fingerprint
        self.hostnames = hostnames
        self.fullchainPem = fullchainPem
        self.privkeyPem = privkeyPem
    }
}

public struct ServerPage: Codable {
    public let items: [ServerResponse]?
    public let total: String?


    public init(items: [ServerResponse]? = nil, total: String? = nil) {
        self.items = items
        self.total = total
    }
}

public struct AuditLogResponse: Codable {
    public let id: String?
    public let operatorId: String?
    public let operatorType: String?
    public let action: String?
    public let targetType: String?
    public let targetId: String?
    public let targetUuid: String?
    public let ipAddress: String?
    public let changes: [String: Any]?
    public let createdAt: String?


    public init(id: String? = nil, operatorId: String? = nil, operatorType: String? = nil, action: String? = nil, targetType: String? = nil, targetId: String? = nil, targetUuid: String? = nil, ipAddress: String? = nil, changes: [String: Any]? = nil, createdAt: String? = nil) {
        self.id = id
        self.operatorId = operatorId
        self.operatorType = operatorType
        self.action = action
        self.targetType = targetType
        self.targetId = targetId
        self.targetUuid = targetUuid
        self.ipAddress = ipAddress
        self.changes = changes
        self.createdAt = createdAt
    }
}

public struct AuditLogPage: Codable {
    public let items: [AuditLogResponse]?
    public let total: String?


    public init(items: [AuditLogResponse]? = nil, total: String? = nil) {
        self.items = items
        self.total = total
    }
}

public struct SdkWorkApiResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct SdkWorkResourceData: Codable {
    public let item: [String: Any]?


    public init(item: [String: Any]? = nil) {
        self.item = item
    }
}

public struct SdkWorkPageData: Codable {
    public let items: [[String: Any]]?
    public let pageInfo: PageInfo?


    public init(items: [[String: Any]]? = nil, pageInfo: PageInfo? = nil) {
        self.items = items
        self.pageInfo = pageInfo
    }
}

public struct SdkWorkCommandData: Codable {
    public let accepted: Bool?
    public let resourceId: String?
    public let status: String?


    public init(accepted: Bool? = nil, resourceId: String? = nil, status: String? = nil) {
        self.accepted = accepted
        self.resourceId = resourceId
        self.status = status
    }
}

public struct SdkWorkAsyncData: Codable {
    public let accepted: Bool?
    public let operationId: String?
    public let status: String?
    public let pollUrl: String?


    public init(accepted: Bool? = nil, operationId: String? = nil, status: String? = nil, pollUrl: String? = nil) {
        self.accepted = accepted
        self.operationId = operationId
        self.status = status
        self.pollUrl = pollUrl
    }
}

public struct PageInfo: Codable {
    public let mode: String?
    public let page: Int?
    public let pageSize: Int?
    public let totalItems: String?
    public let totalPages: Int?
    public let nextCursor: String?
    public let hasMore: Bool?


    public init(mode: String? = nil, page: Int? = nil, pageSize: Int? = nil, totalItems: String? = nil, totalPages: Int? = nil, nextCursor: String? = nil, hasMore: Bool? = nil) {
        self.mode = mode
        self.page = page
        self.pageSize = pageSize
        self.totalItems = totalItems
        self.totalPages = totalPages
        self.nextCursor = nextCursor
        self.hasMore = hasMore
    }
}

public struct FieldError: Codable {
    public let field: String?
    public let message: String?
    public let code: Int?


    public init(field: String? = nil, message: String? = nil, code: Int? = nil) {
        self.field = field
        self.message = message
        self.code = code
    }
}

public struct SdkWorkResourceResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct SdkWorkListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct SdkWorkCommandResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClusterResponse: Codable {
    public let id: String?
    public let name: String?
    public let code: String?
    public let description: String?
    public let status: Int?
    public let heartbeatIntervalSeconds: Int?
    public let offlineThresholdSeconds: Int?
    public let hostCount: String?
    public let instanceCount: String?
    public let onlineInstanceCount: String?
    public let lbStrategy: String?
    public let servedDomains: [String]?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, name: String? = nil, code: String? = nil, description: String? = nil, status: Int? = nil, heartbeatIntervalSeconds: Int? = nil, offlineThresholdSeconds: Int? = nil, hostCount: String? = nil, instanceCount: String? = nil, onlineInstanceCount: String? = nil, lbStrategy: String? = nil, servedDomains: [String]? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.name = name
        self.code = code
        self.description = description
        self.status = status
        self.heartbeatIntervalSeconds = heartbeatIntervalSeconds
        self.offlineThresholdSeconds = offlineThresholdSeconds
        self.hostCount = hostCount
        self.instanceCount = instanceCount
        self.onlineInstanceCount = onlineInstanceCount
        self.lbStrategy = lbStrategy
        self.servedDomains = servedDomains
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct CreateClusterRequest: Codable {
    public let name: String?
    public let code: String?
    public let description: String?
    public let heartbeatIntervalSeconds: Int?
    public let offlineThresholdSeconds: Int?
    public let lbStrategy: String?
    public let servedDomains: [String]?


    public init(name: String? = nil, code: String? = nil, description: String? = nil, heartbeatIntervalSeconds: Int? = nil, offlineThresholdSeconds: Int? = nil, lbStrategy: String? = nil, servedDomains: [String]? = nil) {
        self.name = name
        self.code = code
        self.description = description
        self.heartbeatIntervalSeconds = heartbeatIntervalSeconds
        self.offlineThresholdSeconds = offlineThresholdSeconds
        self.lbStrategy = lbStrategy
        self.servedDomains = servedDomains
    }
}

public struct UpdateClusterRequest: Codable {
    public let name: String?
    public let description: String?
    public let status: Int?
    public let heartbeatIntervalSeconds: Int?
    public let offlineThresholdSeconds: Int?
    public let lbStrategy: String?
    public let servedDomains: [String]?


    public init(name: String? = nil, description: String? = nil, status: Int? = nil, heartbeatIntervalSeconds: Int? = nil, offlineThresholdSeconds: Int? = nil, lbStrategy: String? = nil, servedDomains: [String]? = nil) {
        self.name = name
        self.description = description
        self.status = status
        self.heartbeatIntervalSeconds = heartbeatIntervalSeconds
        self.offlineThresholdSeconds = offlineThresholdSeconds
        self.lbStrategy = lbStrategy
        self.servedDomains = servedDomains
    }
}

public struct ClusterHostResponse: Codable {
    public let id: String?
    public let clusterId: String?
    public let name: String?
    public let hostname: String?
    public let machineCode: String?
    public let osName: String?
    public let osVersion: String?
    public let kernelVersion: String?
    public let arch: String?
    public let cpuModel: String?
    public let cpuCores: Int?
    public let memoryTotalMb: String?
    public let remoteIp: String?
    public let localIps: [String]?
    public let macAddresses: [String]?
    public let daemonVersion: String?
    public let status: Int?
    public let lastHeartbeatAt: String?
    public let instanceCount: String?
    public let joinMode: String?
    public let tunnelRouteDomain: String?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, clusterId: String? = nil, name: String? = nil, hostname: String? = nil, machineCode: String? = nil, osName: String? = nil, osVersion: String? = nil, kernelVersion: String? = nil, arch: String? = nil, cpuModel: String? = nil, cpuCores: Int? = nil, memoryTotalMb: String? = nil, remoteIp: String? = nil, localIps: [String]? = nil, macAddresses: [String]? = nil, daemonVersion: String? = nil, status: Int? = nil, lastHeartbeatAt: String? = nil, instanceCount: String? = nil, joinMode: String? = nil, tunnelRouteDomain: String? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.clusterId = clusterId
        self.name = name
        self.hostname = hostname
        self.machineCode = machineCode
        self.osName = osName
        self.osVersion = osVersion
        self.kernelVersion = kernelVersion
        self.arch = arch
        self.cpuModel = cpuModel
        self.cpuCores = cpuCores
        self.memoryTotalMb = memoryTotalMb
        self.remoteIp = remoteIp
        self.localIps = localIps
        self.macAddresses = macAddresses
        self.daemonVersion = daemonVersion
        self.status = status
        self.lastHeartbeatAt = lastHeartbeatAt
        self.instanceCount = instanceCount
        self.joinMode = joinMode
        self.tunnelRouteDomain = tunnelRouteDomain
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct UpdateClusterHostRequest: Codable {
    public let name: String?
    public let clusterId: String?


    public init(name: String? = nil, clusterId: String? = nil) {
        self.name = name
        self.clusterId = clusterId
    }
}

public struct ClusterInstanceResponse: Codable {
    public let id: String?
    public let clusterId: String?
    public let hostId: String?
    public let hostName: String?
    public let name: String?
    public let role: String?
    public let environment: String?
    public let processPid: Int?
    public let processStartedAt: String?
    public let bindHost: String?
    public let bindPort: Int?
    public let publicEndpoint: String?
    public let buildVersion: String?
    public let status: Int?
    public let healthState: String?
    public let lastHeartbeatAt: String?
    public let lastOnlineAt: String?
    public let uptimeSeconds: String?
    public let metrics: [String: Any]?
    public let joinMode: String?
    public let qualityScore: Int?
    public let desiredConfigRevision: String?
    public let appliedConfigRevision: String?
    public let desiredApplicationsRevision: String?
    public let appliedApplicationsRevision: String?
    public let syncStatus: String?
    public let routingEnabled: Bool?
    public let draining: Bool?
    public let ejected: Bool?
    public let restartCount: Int?
    public let labels: [String: String]?
    public let routingWeight: Int?
    public let maintenanceNote: String?
    public let probeFailures: Int?
    public let probeUrl: String?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, clusterId: String? = nil, hostId: String? = nil, hostName: String? = nil, name: String? = nil, role: String? = nil, environment: String? = nil, processPid: Int? = nil, processStartedAt: String? = nil, bindHost: String? = nil, bindPort: Int? = nil, publicEndpoint: String? = nil, buildVersion: String? = nil, status: Int? = nil, healthState: String? = nil, lastHeartbeatAt: String? = nil, lastOnlineAt: String? = nil, uptimeSeconds: String? = nil, metrics: [String: Any]? = nil, joinMode: String? = nil, qualityScore: Int? = nil, desiredConfigRevision: String? = nil, appliedConfigRevision: String? = nil, desiredApplicationsRevision: String? = nil, appliedApplicationsRevision: String? = nil, syncStatus: String? = nil, routingEnabled: Bool? = nil, draining: Bool? = nil, ejected: Bool? = nil, restartCount: Int? = nil, labels: [String: String]? = nil, routingWeight: Int? = nil, maintenanceNote: String? = nil, probeFailures: Int? = nil, probeUrl: String? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.clusterId = clusterId
        self.hostId = hostId
        self.hostName = hostName
        self.name = name
        self.role = role
        self.environment = environment
        self.processPid = processPid
        self.processStartedAt = processStartedAt
        self.bindHost = bindHost
        self.bindPort = bindPort
        self.publicEndpoint = publicEndpoint
        self.buildVersion = buildVersion
        self.status = status
        self.healthState = healthState
        self.lastHeartbeatAt = lastHeartbeatAt
        self.lastOnlineAt = lastOnlineAt
        self.uptimeSeconds = uptimeSeconds
        self.metrics = metrics
        self.joinMode = joinMode
        self.qualityScore = qualityScore
        self.desiredConfigRevision = desiredConfigRevision
        self.appliedConfigRevision = appliedConfigRevision
        self.desiredApplicationsRevision = desiredApplicationsRevision
        self.appliedApplicationsRevision = appliedApplicationsRevision
        self.syncStatus = syncStatus
        self.routingEnabled = routingEnabled
        self.draining = draining
        self.ejected = ejected
        self.restartCount = restartCount
        self.labels = labels
        self.routingWeight = routingWeight
        self.maintenanceNote = maintenanceNote
        self.probeFailures = probeFailures
        self.probeUrl = probeUrl
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct UpdateClusterInstanceRequest: Codable {
    public let name: String?
    public let status: Int?
    public let publicEndpoint: String?
    public let routingEnabled: Bool?
    public let draining: Bool?
    public let probeUrl: String?
    public let labels: [String: String]?
    public let routingWeight: Int?
    public let maintenanceNote: String?


    public init(name: String? = nil, status: Int? = nil, publicEndpoint: String? = nil, routingEnabled: Bool? = nil, draining: Bool? = nil, probeUrl: String? = nil, labels: [String: String]? = nil, routingWeight: Int? = nil, maintenanceNote: String? = nil) {
        self.name = name
        self.status = status
        self.publicEndpoint = publicEndpoint
        self.routingEnabled = routingEnabled
        self.draining = draining
        self.probeUrl = probeUrl
        self.labels = labels
        self.routingWeight = routingWeight
        self.maintenanceNote = maintenanceNote
    }
}

public struct ClusterEventResponse: Codable {
    public let id: String?
    public let clusterId: String?
    public let hostId: String?
    public let instanceId: String?
    public let eventType: String?
    public let severity: String?
    public let message: String?
    public let detail: [String: Any]?
    public let occurredAt: String?
    public let createdAt: String?


    public init(id: String? = nil, clusterId: String? = nil, hostId: String? = nil, instanceId: String? = nil, eventType: String? = nil, severity: String? = nil, message: String? = nil, detail: [String: Any]? = nil, occurredAt: String? = nil, createdAt: String? = nil) {
        self.id = id
        self.clusterId = clusterId
        self.hostId = hostId
        self.instanceId = instanceId
        self.eventType = eventType
        self.severity = severity
        self.message = message
        self.detail = detail
        self.occurredAt = occurredAt
        self.createdAt = createdAt
    }
}

public struct ClusterOverviewResponse: Codable {
    public let totalHosts: String?
    public let onlineHosts: String?
    public let totalInstances: String?
    public let onlineInstances: String?
    public let unhealthyInstances: String?
    public let pendingPeerMessages: String?
    public let generatedAt: String?


    public init(totalHosts: String? = nil, onlineHosts: String? = nil, totalInstances: String? = nil, onlineInstances: String? = nil, unhealthyInstances: String? = nil, pendingPeerMessages: String? = nil, generatedAt: String? = nil) {
        self.totalHosts = totalHosts
        self.onlineHosts = onlineHosts
        self.totalInstances = totalInstances
        self.onlineInstances = onlineInstances
        self.unhealthyInstances = unhealthyInstances
        self.pendingPeerMessages = pendingPeerMessages
        self.generatedAt = generatedAt
    }
}

public struct TrafficUsageStatisticsResponse: Codable {
    public let dateFrom: String?
    public let dateTo: String?
    public let platformScope: Bool?
    public let totals: [TrafficUsageTotal]?
    public let daily: [TrafficUsageDailyPoint]?
    public let apps: [TrafficUsageAppTotal]?
    public let tenants: [TrafficUsageTenantTotal]?


    public init(dateFrom: String? = nil, dateTo: String? = nil, platformScope: Bool? = nil, totals: [TrafficUsageTotal]? = nil, daily: [TrafficUsageDailyPoint]? = nil, apps: [TrafficUsageAppTotal]? = nil, tenants: [TrafficUsageTenantTotal]? = nil) {
        self.dateFrom = dateFrom
        self.dateTo = dateTo
        self.platformScope = platformScope
        self.totals = totals
        self.daily = daily
        self.apps = apps
        self.tenants = tenants
    }
}

public struct TrafficUsageTotal: Codable {
    public let dimension: String?
    public let quantity: String?
    public let unit: String?


    public init(dimension: String? = nil, quantity: String? = nil, unit: String? = nil) {
        self.dimension = dimension
        self.quantity = quantity
        self.unit = unit
    }
}

public struct TrafficUsageDailyPoint: Codable {
    public let usageDate: String?
    public let dimension: String?
    public let quantity: String?


    public init(usageDate: String? = nil, dimension: String? = nil, quantity: String? = nil) {
        self.usageDate = usageDate
        self.dimension = dimension
        self.quantity = quantity
    }
}

public struct TrafficUsageAppTotal: Codable {
    public let appUuid: String?
    public let appSlug: String?
    public let dimension: String?
    public let quantity: String?
    public let unit: String?


    public init(appUuid: String? = nil, appSlug: String? = nil, dimension: String? = nil, quantity: String? = nil, unit: String? = nil) {
        self.appUuid = appUuid
        self.appSlug = appSlug
        self.dimension = dimension
        self.quantity = quantity
        self.unit = unit
    }
}

public struct TrafficUsageTenantTotal: Codable {
    public let tenantId: String?
    public let dimension: String?
    public let quantity: String?
    public let unit: String?


    public init(tenantId: String? = nil, dimension: String? = nil, quantity: String? = nil, unit: String? = nil) {
        self.tenantId = tenantId
        self.dimension = dimension
        self.quantity = quantity
        self.unit = unit
    }
}

public struct MetricsSummaryResponse: Codable {
    public let asOf: String?
    public let platformScope: Bool?
    public let trafficSince: String?
    public let windows: [MetricsWindowBounds]?
    public let entities: [MetricsMetricTotals]?
    public let traffic: [MetricsMetricTotals]?
    public let storage: [MetricsMetricTotals]?
    public let series: [MetricsSeries]?
    public let seriesWindow: MetricsSeriesWindow?
    public let unassembledMetrics: [String]?


    public init(asOf: String? = nil, platformScope: Bool? = nil, trafficSince: String? = nil, windows: [MetricsWindowBounds]? = nil, entities: [MetricsMetricTotals]? = nil, traffic: [MetricsMetricTotals]? = nil, storage: [MetricsMetricTotals]? = nil, series: [MetricsSeries]? = nil, seriesWindow: MetricsSeriesWindow? = nil, unassembledMetrics: [String]? = nil) {
        self.asOf = asOf
        self.platformScope = platformScope
        self.trafficSince = trafficSince
        self.windows = windows
        self.entities = entities
        self.traffic = traffic
        self.storage = storage
        self.series = series
        self.seriesWindow = seriesWindow
        self.unassembledMetrics = unassembledMetrics
    }
}

public struct MetricsWindowBounds: Codable {
    public let window: String?
    public let dateFrom: String?
    public let dateTo: String?


    public init(window: String? = nil, dateFrom: String? = nil, dateTo: String? = nil) {
        self.window = window
        self.dateFrom = dateFrom
        self.dateTo = dateTo
    }
}

public struct MetricsMetricTotals: Codable {
    public let metric: String?
    public let unit: String?
    public let values: [MetricsWindowValue]?


    public init(metric: String? = nil, unit: String? = nil, values: [MetricsWindowValue]? = nil) {
        self.metric = metric
        self.unit = unit
        self.values = values
    }
}

public struct MetricsWindowValue: Codable {
    public let window: String?
    public let quantity: String?
    public let unit: String?


    public init(window: String? = nil, quantity: String? = nil, unit: String? = nil) {
        self.window = window
        self.quantity = quantity
        self.unit = unit
    }
}

public struct MetricsSeriesWindow: Codable {
    public let dateFrom: String?
    public let dateTo: String?


    public init(dateFrom: String? = nil, dateTo: String? = nil) {
        self.dateFrom = dateFrom
        self.dateTo = dateTo
    }
}

public struct MetricsSeries: Codable {
    public let metric: String?
    public let unit: String?
    public let points: [MetricsSeriesPoint]?


    public init(metric: String? = nil, unit: String? = nil, points: [MetricsSeriesPoint]? = nil) {
        self.metric = metric
        self.unit = unit
        self.points = points
    }
}

public struct MetricsSeriesPoint: Codable {
    public let date: String?
    public let quantity: String?


    public init(date: String? = nil, quantity: String? = nil) {
        self.date = date
        self.quantity = quantity
    }
}

public struct ApplicationsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsActivateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsPauseResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDomainsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDomainsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDomainsVerifyResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDomainsListenerCertificateBindingsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDomainsListenerCertificateBindingsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct RootDomainsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct RootDomainsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct RootDomainsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct RootDomainsUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct RootDomainsSubdomainsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct RootDomainsSubdomainsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct DomainsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct DomainsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct DomainsVerifyResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct DomainsApplicationBindingUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsSourceVersionsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsSourceVersionsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsSourceVersionsGitImportCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsSourceVersionsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDeploymentsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct CertificatesListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct CertificatesIssueResponse202: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct CertificatesOperationsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct CertificatesUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct CertificatesRenewResponse202: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct CertificatesRevokeResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct CertificatesDistributionListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ConfigsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ConfigsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ConfigsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ConfigsUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ConfigsValidateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ConfigsDeployResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ReloadResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct StatusRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ServersListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ServersCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ServerFilesNodesListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ServerFilesNodeDirectoryListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ServerFilesNodeRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ServerFilesNodeOperationsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ServerFilesNodeOperationsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct WebserverConfigsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct WebserverConfigsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct WebserverConfigsUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct HeartbeatResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct RetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct AuditLogsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersSyncResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersHostsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersHostsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersHostsUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesUpdateResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersEventsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersOverviewRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesHeartbeatsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesMetricsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesProbeResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesDrainResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesUndrainResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesCordonResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersInstancesUncordonResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ClustersMessagesCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct TrafficUsagesRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct PlatformTrafficUsagesRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct MetricsSummariesRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct PlatformMetricsSummariesRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}
