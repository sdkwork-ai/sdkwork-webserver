module Sdkwork
  module AppSdk
    module Models
      class HealthCheckResponse
              attr_accessor :id, :check_type, :check_url, :check_interval, :timeout_ms, :retry_count, :status, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @check_type = attributes['checkType']
                @check_url = attributes['checkUrl']
                @check_interval = attributes['checkInterval']
                @timeout_ms = attributes['timeoutMs']
                @retry_count = attributes['retryCount']
                @status = attributes['status']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'checkType' => @check_type,
                  'checkUrl' => @check_url,
                  'checkInterval' => @check_interval,
                  'timeoutMs' => @timeout_ms,
                  'retryCount' => @retry_count,
                  'status' => @status,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
