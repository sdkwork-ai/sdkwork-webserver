module Sdkwork
  module BackendSdk
    module Models
      class ClusterSyncManifest
              attr_accessor :cluster_id, :kind, :revision, :sha256, :payload, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @cluster_id = attributes['clusterId']
                @kind = attributes['kind']
                @revision = attributes['revision']
                @sha256 = attributes['sha256']
                @payload = attributes['payload'].is_a?(Hash) ? attributes['payload'] : {}
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'clusterId' => @cluster_id,
                  'kind' => @kind,
                  'revision' => @revision,
                  'sha256' => @sha256,
                  'payload' => @payload,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
