module Sdkwork
  module BackendSdk
    module Models
      class CreateApplicationSourceVersionRequest
              attr_accessor :version_tag, :source_type, :source_ref, :commit_hash, :artifact_drive_uri, :artifact_size, :artifact_hash, :config_snapshot

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @version_tag = attributes['versionTag']
                @source_type = attributes['sourceType']
                @source_ref = attributes['sourceRef']
                @commit_hash = attributes['commitHash']
                @artifact_drive_uri = attributes['artifactDriveUri']
                @artifact_size = attributes['artifactSize']
                @artifact_hash = attributes['artifactHash']
                @config_snapshot = attributes['configSnapshot'].is_a?(Hash) ? ApplicationSourceVersionConfigSnapshot.from_hash(attributes['configSnapshot']) : nil
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'versionTag' => @version_tag,
                  'sourceType' => @source_type,
                  'sourceRef' => @source_ref,
                  'commitHash' => @commit_hash,
                  'artifactDriveUri' => @artifact_drive_uri,
                  'artifactSize' => @artifact_size,
                  'artifactHash' => @artifact_hash,
                  'configSnapshot' => @config_snapshot&.to_hash,
                }
              end
            end
    end
  end
end
