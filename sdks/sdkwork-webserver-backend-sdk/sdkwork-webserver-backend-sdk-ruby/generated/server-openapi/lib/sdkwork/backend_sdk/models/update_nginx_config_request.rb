module Sdkwork
  module BackendSdk
    module Models
      class UpdateNginxConfigRequest
              attr_accessor :config_content, :config_name

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @config_content = attributes['configContent']
                @config_name = attributes['configName']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'configContent' => @config_content,
                  'configName' => @config_name,
                }
              end
            end
    end
  end
end
