module Sdkwork
  module AppSdk
    module Models
      class EnvVariableResponse
              attr_accessor :id, :key, :environment, :is_secret, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @key = attributes['key']
                @environment = attributes['environment']
                @is_secret = attributes['isSecret']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'key' => @key,
                  'environment' => @environment,
                  'isSecret' => @is_secret,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
