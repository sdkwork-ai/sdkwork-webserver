module Sdkwork
  module BackendSdk
    module Models
      class PublishClusterSyncRequest
              attr_accessor :kind, :payload

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @kind = attributes['kind']
                @payload = attributes['payload'].is_a?(Hash) ? attributes['payload'] : {}
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'kind' => @kind,
                  'payload' => @payload,
                }
              end
            end
    end
  end
end
