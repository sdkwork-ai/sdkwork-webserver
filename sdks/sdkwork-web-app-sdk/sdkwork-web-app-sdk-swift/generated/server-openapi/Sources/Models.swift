import Foundation

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
    public let siteId: String?
    public let appKind: String?
    public let siteType: Int?
    public let status: Int?
    public let runtimeConfig: [String: Any]?
    public let storeListing: ApplicationStoreListing?
    public let createdAt: String?
    public let updatedAt: String?


    public init(id: String? = nil, name: String? = nil, slug: String? = nil, description: String? = nil, siteId: String? = nil, appKind: String? = nil, siteType: Int? = nil, status: Int? = nil, runtimeConfig: [String: Any]? = nil, storeListing: ApplicationStoreListing? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.name = name
        self.slug = slug
        self.description = description
        self.siteId = siteId
        self.appKind = appKind
        self.siteType = siteType
        self.status = status
        self.runtimeConfig = runtimeConfig
        self.storeListing = storeListing
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct ApplicationPage: Codable {
    public let items: [ApplicationResponse]?
    public let total: String?
    public let page: Int?
    public let pageSize: Int?


    public init(items: [ApplicationResponse]? = nil, total: String? = nil, page: Int? = nil, pageSize: Int? = nil) {
        self.items = items
        self.total = total
        self.page = page
        self.pageSize = pageSize
    }
}

public struct CreateDomainRequest: Codable {
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

public struct DomainResponse: Codable {
    public let id: String?
    public let hostname: String?
    public let applicationId: String?
    public let applicationName: String?
    public let certificateCount: String?
    public let isPrimary: Bool?
    public let isVerified: Bool?
    public let sslEnabled: Bool?
    public let sslProvider: String?
    public let status: Int?
    public let createdAt: String?


    public init(id: String? = nil, hostname: String? = nil, applicationId: String? = nil, applicationName: String? = nil, certificateCount: String? = nil, isPrimary: Bool? = nil, isVerified: Bool? = nil, sslEnabled: Bool? = nil, sslProvider: String? = nil, status: Int? = nil, createdAt: String? = nil) {
        self.id = id
        self.hostname = hostname
        self.applicationId = applicationId
        self.applicationName = applicationName
        self.certificateCount = certificateCount
        self.isPrimary = isPrimary
        self.isVerified = isVerified
        self.sslEnabled = sslEnabled
        self.sslProvider = sslProvider
        self.status = status
        self.createdAt = createdAt
    }
}

public struct DomainPage: Codable {
    public let items: [DomainResponse]?
    public let total: String?


    public init(items: [DomainResponse]? = nil, total: String? = nil) {
        self.items = items
        self.total = total
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

public struct SourceVersionConfigSnapshot: Codable {
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

public struct CreateSourceVersionRequest: Codable {
    public let versionTag: String?
    public let sourceType: String?
    public let sourceRef: String?
    public let commitHash: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?
    public let configSnapshot: SourceVersionConfigSnapshot?


    public init(versionTag: String? = nil, sourceType: String? = nil, sourceRef: String? = nil, commitHash: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil, configSnapshot: SourceVersionConfigSnapshot? = nil) {
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

public struct ImportGitSourceVersionRequest: Codable {
    public let versionTag: String?
    public let repositoryUrl: String?
    public let gitRef: String?


    public init(versionTag: String? = nil, repositoryUrl: String? = nil, gitRef: String? = nil) {
        self.versionTag = versionTag
        self.repositoryUrl = repositoryUrl
        self.gitRef = gitRef
    }
}

public struct SourceVersionResponse: Codable {
    public let id: String?
    public let applicationId: String?
    public let versionTag: String?
    public let sourceType: String?
    public let sourceRef: String?
    public let commitHash: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?
    public let configSnapshot: SourceVersionConfigSnapshot?
    public let status: Int?
    public let retained: Bool?
    public let createdAt: String?


    public init(id: String? = nil, applicationId: String? = nil, versionTag: String? = nil, sourceType: String? = nil, sourceRef: String? = nil, commitHash: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil, configSnapshot: SourceVersionConfigSnapshot? = nil, status: Int? = nil, retained: Bool? = nil, createdAt: String? = nil) {
        self.id = id
        self.applicationId = applicationId
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

public struct SourceVersionPage: Codable {
    public let items: [SourceVersionResponse]?
    public let total: String?


    public init(items: [SourceVersionResponse]? = nil, total: String? = nil) {
        self.items = items
        self.total = total
    }
}

public struct CreateDeploymentRequest: Codable {
    public let sourceVersionId: String?
    public let deployType: Int?
    public let versionTag: String?
    public let commitHash: String?
    public let sourceRef: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?
    public let environment: String?


    public init(sourceVersionId: String? = nil, deployType: Int? = nil, versionTag: String? = nil, commitHash: String? = nil, sourceRef: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil, environment: String? = nil) {
        self.sourceVersionId = sourceVersionId
        self.deployType = deployType
        self.versionTag = versionTag
        self.commitHash = commitHash
        self.sourceRef = sourceRef
        self.artifactDriveUri = artifactDriveUri
        self.artifactSize = artifactSize
        self.artifactHash = artifactHash
        self.environment = environment
    }
}

public struct DeploymentResponse: Codable {
    public let id: String?
    public let applicationId: String?
    public let deployType: Int?
    public let sourceVersionId: String?
    public let versionTag: String?
    public let commitHash: String?
    public let sourceRef: String?
    public let rollbackFromDeploymentId: String?
    public let environment: String?
    public let artifactDriveUri: String?
    public let artifactSize: String?
    public let artifactHash: String?
    public let status: Int?
    public let startedAt: String?
    public let completedAt: String?
    public let durationMs: String?
    public let createdAt: String?


    public init(id: String? = nil, applicationId: String? = nil, deployType: Int? = nil, sourceVersionId: String? = nil, versionTag: String? = nil, commitHash: String? = nil, sourceRef: String? = nil, rollbackFromDeploymentId: String? = nil, environment: String? = nil, artifactDriveUri: String? = nil, artifactSize: String? = nil, artifactHash: String? = nil, status: Int? = nil, startedAt: String? = nil, completedAt: String? = nil, durationMs: String? = nil, createdAt: String? = nil) {
        self.id = id
        self.applicationId = applicationId
        self.deployType = deployType
        self.sourceVersionId = sourceVersionId
        self.versionTag = versionTag
        self.commitHash = commitHash
        self.sourceRef = sourceRef
        self.rollbackFromDeploymentId = rollbackFromDeploymentId
        self.environment = environment
        self.artifactDriveUri = artifactDriveUri
        self.artifactSize = artifactSize
        self.artifactHash = artifactHash
        self.status = status
        self.startedAt = startedAt
        self.completedAt = completedAt
        self.durationMs = durationMs
        self.createdAt = createdAt
    }
}

public struct DeploymentPage: Codable {
    public let items: [DeploymentResponse]?
    public let total: String?


    public init(items: [DeploymentResponse]? = nil, total: String? = nil) {
        self.items = items
        self.total = total
    }
}

public struct CreateEnvVariableRequest: Codable {
    public let key: String?
    public let value: String?
    public let environment: String?
    public let isSecret: Bool?


    public init(key: String? = nil, value: String? = nil, environment: String? = nil, isSecret: Bool? = nil) {
        self.key = key
        self.value = value
        self.environment = environment
        self.isSecret = isSecret
    }
}

public struct UpdateEnvVariableRequest: Codable {
    public let value: String?
    public let isSecret: Bool?


    public init(value: String? = nil, isSecret: Bool? = nil) {
        self.value = value
        self.isSecret = isSecret
    }
}

public struct EnvVariableResponse: Codable {
    public let id: String?
    public let key: String?
    public let environment: String?
    public let isSecret: Bool?
    public let createdAt: String?


    public init(id: String? = nil, key: String? = nil, environment: String? = nil, isSecret: Bool? = nil, createdAt: String? = nil) {
        self.id = id
        self.key = key
        self.environment = environment
        self.isSecret = isSecret
        self.createdAt = createdAt
    }
}

public struct EnvVariablePage: Codable {
    public let items: [EnvVariableResponse]?
    public let total: String?


    public init(items: [EnvVariableResponse]? = nil, total: String? = nil) {
        self.items = items
        self.total = total
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
    public let applicationId: String?
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


    public init(id: String? = nil, applicationId: String? = nil, domainId: String? = nil, certificateId: String? = nil, desiredCertificateVersionId: String? = nil, currentCertificateVersionId: String? = nil, desiredCertificate: ListenerCertificateSummaryResponse? = nil, currentCertificate: ListenerCertificateSummaryResponse? = nil, keyAlgorithm: String? = nil, priority: Int? = nil, isDefault: Bool? = nil, status: String? = nil, activatedAt: String? = nil, createdAt: String? = nil, updatedAt: String? = nil) {
        self.id = id
        self.applicationId = applicationId
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

public struct CreateHealthCheckRequest: Codable {
    public let checkType: Int?
    public let checkUrl: String?
    public let checkInterval: Int?
    public let timeoutMs: Int?
    public let retryCount: Int?


    public init(checkType: Int? = nil, checkUrl: String? = nil, checkInterval: Int? = nil, timeoutMs: Int? = nil, retryCount: Int? = nil) {
        self.checkType = checkType
        self.checkUrl = checkUrl
        self.checkInterval = checkInterval
        self.timeoutMs = timeoutMs
        self.retryCount = retryCount
    }
}

public struct HealthCheckResponse: Codable {
    public let id: String?
    public let checkType: Int?
    public let checkUrl: String?
    public let checkInterval: Int?
    public let timeoutMs: Int?
    public let retryCount: Int?
    public let status: Int?
    public let createdAt: String?


    public init(id: String? = nil, checkType: Int? = nil, checkUrl: String? = nil, checkInterval: Int? = nil, timeoutMs: Int? = nil, retryCount: Int? = nil, status: Int? = nil, createdAt: String? = nil) {
        self.id = id
        self.checkType = checkType
        self.checkUrl = checkUrl
        self.checkInterval = checkInterval
        self.timeoutMs = timeoutMs
        self.retryCount = retryCount
        self.status = status
        self.createdAt = createdAt
    }
}

public struct HealthCheckPage: Codable {
    public let items: [HealthCheckResponse]?
    public let total: String?


    public init(items: [HealthCheckResponse]? = nil, total: String? = nil) {
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

public struct ApplicationsDomainsRetrieveResponse: Codable {
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

public struct ApplicationsDeploymentsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDeploymentsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsDeploymentsRollbackResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsEnvVariablesListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsEnvVariablesCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsEnvVariablesUpdateResponse: Codable {
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

public struct ApplicationsPlatformTargetsListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsPlatformTargetsCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsPlatformTargetsRetrieveResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsHealthChecksListResponse: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}

public struct ApplicationsHealthChecksCreateResponse201: Codable {
    public let code: Int?
    public let data: Any?
    public let traceId: String?


    public init(code: Int? = nil, data: Any? = nil, traceId: String? = nil) {
        self.code = code
        self.data = data
        self.traceId = traceId
    }
}
