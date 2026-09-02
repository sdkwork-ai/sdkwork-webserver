module Sdkwork
  module BackendSdk
    module Models
      class CreateNginxConfigRequest
              attr_accessor :config_type, :config_name, :config_content, :site_id

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @config_type = attributes['configType']
                @config_name = attributes['configName']
                @config_content = attributes['configContent']
                @site_id = attributes['siteId']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'configType' => @config_type,
                  'configName' => @config_name,
                  'configContent' => @config_content,
                  'siteId' => @site_id,
                }
              end
            end
    end
  end
end
