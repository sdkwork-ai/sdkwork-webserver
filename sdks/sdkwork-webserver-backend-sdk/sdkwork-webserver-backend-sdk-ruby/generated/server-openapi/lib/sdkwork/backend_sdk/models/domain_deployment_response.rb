module Sdkwork
  module BackendSdk
    module Models
      class DomainDeploymentResponse
              attr_accessor :id, :status, :environment, :version_tag, :completed_at, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @status = attributes['status']
                @environment = attributes['environment']
                @version_tag = attributes['versionTag']
                @completed_at = attributes['completedAt']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'status' => @status,
                  'environment' => @environment,
                  'versionTag' => @version_tag,
                  'completedAt' => @completed_at,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
