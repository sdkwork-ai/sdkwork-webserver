module Sdkwork
  module AppSdk
    module Models
      class CreateHealthCheckRequest
              attr_accessor :check_type, :check_url, :check_interval, :timeout_ms, :retry_count

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @check_type = attributes['checkType']
                @check_url = attributes['checkUrl']
                @check_interval = attributes['checkInterval']
                @timeout_ms = attributes['timeoutMs']
                @retry_count = attributes['retryCount']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'checkType' => @check_type,
                  'checkUrl' => @check_url,
                  'checkInterval' => @check_interval,
                  'timeoutMs' => @timeout_ms,
                  'retryCount' => @retry_count,
                }
              end
            end
    end
  end
end
