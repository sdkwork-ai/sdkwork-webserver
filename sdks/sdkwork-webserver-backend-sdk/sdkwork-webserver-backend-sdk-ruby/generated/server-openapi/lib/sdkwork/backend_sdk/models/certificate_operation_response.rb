module Sdkwork
  module BackendSdk
    module Models
      class CertificateOperationResponse
              attr_accessor :id, :certificate_id, :operation_type, :status, :attempt_count, :max_attempts, :next_attempt_at, :failure_code, :created_at, :updated_at, :completed_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @certificate_id = attributes['certificateId']
                @operation_type = attributes['operationType']
                @status = attributes['status']
                @attempt_count = attributes['attemptCount']
                @max_attempts = attributes['maxAttempts']
                @next_attempt_at = attributes['nextAttemptAt']
                @failure_code = attributes['failureCode']
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
                @completed_at = attributes['completedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'certificateId' => @certificate_id,
                  'operationType' => @operation_type,
                  'status' => @status,
                  'attemptCount' => @attempt_count,
                  'maxAttempts' => @max_attempts,
                  'nextAttemptAt' => @next_attempt_at,
                  'failureCode' => @failure_code,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                  'completedAt' => @completed_at,
                }
              end
            end
    end
  end
end
