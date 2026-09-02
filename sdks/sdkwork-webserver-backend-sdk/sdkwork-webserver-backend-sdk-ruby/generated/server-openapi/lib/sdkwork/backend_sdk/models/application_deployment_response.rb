module Sdkwork
  module BackendSdk
    module Models
      class ApplicationDeploymentResponse
              attr_accessor :id, :site_id, :source_version_id, :status, :deploy_type, :environment, :version_tag, :commit_hash, :source_ref, :rollback_from_deployment_id, :artifact_drive_uri, :artifact_size, :artifact_hash, :started_at, :completed_at, :duration_ms, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @site_id = attributes['siteId']
                @source_version_id = attributes['sourceVersionId']
                @status = attributes['status']
                @deploy_type = attributes['deployType']
                @environment = attributes['environment']
                @version_tag = attributes['versionTag']
                @commit_hash = attributes['commitHash']
                @source_ref = attributes['sourceRef']
                @rollback_from_deployment_id = attributes['rollbackFromDeploymentId']
                @artifact_drive_uri = attributes['artifactDriveUri']
                @artifact_size = attributes['artifactSize']
                @artifact_hash = attributes['artifactHash']
                @started_at = attributes['startedAt']
                @completed_at = attributes['completedAt']
                @duration_ms = attributes['durationMs']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'siteId' => @site_id,
                  'sourceVersionId' => @source_version_id,
                  'status' => @status,
                  'deployType' => @deploy_type,
                  'environment' => @environment,
                  'versionTag' => @version_tag,
                  'commitHash' => @commit_hash,
                  'sourceRef' => @source_ref,
                  'rollbackFromDeploymentId' => @rollback_from_deployment_id,
                  'artifactDriveUri' => @artifact_drive_uri,
                  'artifactSize' => @artifact_size,
                  'artifactHash' => @artifact_hash,
                  'startedAt' => @started_at,
                  'completedAt' => @completed_at,
                  'durationMs' => @duration_ms,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
