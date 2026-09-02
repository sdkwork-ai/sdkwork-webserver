module Sdkwork
  module AppSdk
    module Models
      class SourceVersionConfigSnapshot
              attr_accessor :app_config_path, :deployment_config_path, :app_config_detected, :deployment_config_detected

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @app_config_path = attributes['appConfigPath']
                @deployment_config_path = attributes['deploymentConfigPath']
                @app_config_detected = attributes['appConfigDetected']
                @deployment_config_detected = attributes['deploymentConfigDetected']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'appConfigPath' => @app_config_path,
                  'deploymentConfigPath' => @deployment_config_path,
                  'appConfigDetected' => @app_config_detected,
                  'deploymentConfigDetected' => @deployment_config_detected,
                }
              end
            end
    end
  end
end
