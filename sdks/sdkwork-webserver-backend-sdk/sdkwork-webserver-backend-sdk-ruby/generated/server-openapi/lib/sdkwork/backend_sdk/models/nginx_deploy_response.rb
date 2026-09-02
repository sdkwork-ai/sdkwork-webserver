module Sdkwork
  module BackendSdk
    module Models
      class NginxDeployResponse
              attr_accessor :success, :config_id, :deployed_at, :reload_result

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @success = attributes['success']
                @config_id = attributes['configId']
                @deployed_at = attributes['deployedAt']
                @reload_result = attributes['reloadResult'].is_a?(Hash) ? attributes['reloadResult'] : {}
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'success' => @success,
                  'configId' => @config_id,
                  'deployedAt' => @deployed_at,
                  'reloadResult' => @reload_result,
                }
              end
            end
    end
  end
end
