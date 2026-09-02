module Sdkwork
  module BackendSdk
    module Models
      class DomainVerifyResponse
              attr_accessor :verified, :status, :method, :record_name, :record_value, :attempt_count, :expires_at, :next_attempt_at, :checked_at, :failure_code

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @verified = attributes['verified']
                @status = attributes['status']
                @method = attributes['method']
                @record_name = attributes['recordName']
                @record_value = attributes['recordValue']
                @attempt_count = attributes['attemptCount']
                @expires_at = attributes['expiresAt']
                @next_attempt_at = attributes['nextAttemptAt']
                @checked_at = attributes['checkedAt']
                @failure_code = attributes['failureCode']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'verified' => @verified,
                  'status' => @status,
                  'method' => @method,
                  'recordName' => @record_name,
                  'recordValue' => @record_value,
                  'attemptCount' => @attempt_count,
                  'expiresAt' => @expires_at,
                  'nextAttemptAt' => @next_attempt_at,
                  'checkedAt' => @checked_at,
                  'failureCode' => @failure_code,
                }
              end
            end
    end
  end
end
