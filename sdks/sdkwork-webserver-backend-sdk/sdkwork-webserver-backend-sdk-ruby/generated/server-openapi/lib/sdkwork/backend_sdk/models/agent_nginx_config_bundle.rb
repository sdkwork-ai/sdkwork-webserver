module Sdkwork
  module BackendSdk
    module Models
      class AgentNginxConfigBundle
              attr_accessor :config_id, :domain, :config_content, :fingerprint, :version

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @config_id = attributes['configId']
                @domain = attributes['domain']
                @config_content = attributes['configContent']
                @fingerprint = attributes['fingerprint']
                @version = attributes['version']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'configId' => @config_id,
                  'domain' => @domain,
                  'configContent' => @config_content,
                  'fingerprint' => @fingerprint,
                  'version' => @version,
                }
              end
            end
    end
  end
end
