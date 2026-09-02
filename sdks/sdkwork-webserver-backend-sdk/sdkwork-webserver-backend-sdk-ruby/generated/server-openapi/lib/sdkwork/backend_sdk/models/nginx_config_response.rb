module Sdkwork
  module BackendSdk
    module Models
      class NginxConfigResponse
              attr_accessor :id, :config_type, :config_name, :config_content, :config_hash, :is_active, :version_no, :deployed_at, :status, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @config_type = attributes['configType']
                @config_name = attributes['configName']
                @config_content = attributes['configContent']
                @config_hash = attributes['configHash']
                @is_active = attributes['isActive']
                @version_no = attributes['versionNo']
                @deployed_at = attributes['deployedAt']
                @status = attributes['status']
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'configType' => @config_type,
                  'configName' => @config_name,
                  'configContent' => @config_content,
                  'configHash' => @config_hash,
                  'isActive' => @is_active,
                  'versionNo' => @version_no,
                  'deployedAt' => @deployed_at,
                  'status' => @status,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end
