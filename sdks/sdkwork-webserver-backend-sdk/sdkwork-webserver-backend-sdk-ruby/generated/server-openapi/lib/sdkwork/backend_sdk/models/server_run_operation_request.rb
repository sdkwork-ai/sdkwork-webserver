module Sdkwork
  module BackendSdk
    module Models
      class ServerRunOperationRequest
              attr_accessor :path, :operation_id

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @path = attributes['path']
                @operation_id = attributes['operationId']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'path' => @path,
                  'operationId' => @operation_id,
                }
              end
            end
    end
  end
end
