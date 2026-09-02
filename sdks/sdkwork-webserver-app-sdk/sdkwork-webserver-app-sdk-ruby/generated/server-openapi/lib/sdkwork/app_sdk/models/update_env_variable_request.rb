module Sdkwork
  module AppSdk
    module Models
      class UpdateEnvVariableRequest
              attr_accessor :value, :is_secret

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @value = attributes['value']
                @is_secret = attributes['isSecret']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'value' => @value,
                  'isSecret' => @is_secret,
                }
              end
            end
    end
  end
end
