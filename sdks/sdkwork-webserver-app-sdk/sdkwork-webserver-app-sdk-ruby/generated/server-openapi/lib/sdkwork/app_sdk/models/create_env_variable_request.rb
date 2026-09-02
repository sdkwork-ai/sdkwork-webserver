module Sdkwork
  module AppSdk
    module Models
      class CreateEnvVariableRequest
              attr_accessor :key, :value, :environment, :is_secret

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @key = attributes['key']
                @value = attributes['value']
                @environment = attributes['environment']
                @is_secret = attributes['isSecret']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'key' => @key,
                  'value' => @value,
                  'environment' => @environment,
                  'isSecret' => @is_secret,
                }
              end
            end
    end
  end
end
