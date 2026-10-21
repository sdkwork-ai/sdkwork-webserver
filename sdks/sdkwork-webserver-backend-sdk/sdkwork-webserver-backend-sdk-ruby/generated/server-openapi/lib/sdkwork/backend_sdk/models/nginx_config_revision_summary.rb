module Sdkwork
  module BackendSdk
    module Models
      class NginxConfigRevisionSummary
              attr_accessor :version_no, :config_name, :config_hash, :created_by, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @version_no = attributes['versionNo']
                @config_name = attributes['configName']
                @config_hash = attributes['configHash']
                @created_by = attributes['createdBy']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'versionNo' => @version_no,
                  'configName' => @config_name,
                  'configHash' => @config_hash,
                  'createdBy' => @created_by,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
