module Sdkwork
  module BackendSdk
    module Models
      class ServerOperationResult
              attr_accessor :operation_id, :exit_code, :stdout, :stderr

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @operation_id = attributes['operationId']
                @exit_code = attributes['exitCode']
                @stdout = attributes['stdout']
                @stderr = attributes['stderr']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'operationId' => @operation_id,
                  'exitCode' => @exit_code,
                  'stdout' => @stdout,
                  'stderr' => @stderr,
                }
              end
            end
    end
  end
end
