module Sdkwork
  module BackendSdk
    module Models
      class ProbeClusterInstanceRequest
              attr_accessor :path, :timeout_ms

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @path = attributes['path']
                @timeout_ms = attributes['timeoutMs']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'path' => @path,
                  'timeoutMs' => @timeout_ms,
                }
              end
            end
    end
  end
end
