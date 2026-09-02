module Sdkwork
  module AppSdk
    module Models
      class DeploymentResponse
              attr_accessor :id, :application_id, :deploy_type, :source_version_id, :version_tag, :commit_hash, :source_ref, :rollback_from_deployment_id, :environment, :artifact_drive_uri, :artifact_size, :artifact_hash, :status, :started_at, :completed_at, :duration_ms, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @application_id = attributes['applicationId']
                @deploy_type = attributes['deployType']
                @source_version_id = attributes['sourceVersionId']
                @version_tag = attributes['versionTag']
                @commit_hash = attributes['commitHash']
                @source_ref = attributes['sourceRef']
                @rollback_from_deployment_id = attributes['rollbackFromDeploymentId']
                @environment = attributes['environment']
                @artifact_drive_uri = attributes['artifactDriveUri']
                @artifact_size = attributes['artifactSize']
                @artifact_hash = attributes['artifactHash']
                @status = attributes['status']
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
                  'applicationId' => @application_id,
                  'deployType' => @deploy_type,
                  'sourceVersionId' => @source_version_id,
                  'versionTag' => @version_tag,
                  'commitHash' => @commit_hash,
                  'sourceRef' => @source_ref,
                  'rollbackFromDeploymentId' => @rollback_from_deployment_id,
                  'environment' => @environment,
                  'artifactDriveUri' => @artifact_drive_uri,
                  'artifactSize' => @artifact_size,
                  'artifactHash' => @artifact_hash,
                  'status' => @status,
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
