module Sdkwork
  module BackendSdk
    module Models
      class NginxReloadResponse
              attr_accessor :success, :message, :timestamp

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @success = attributes['success']
                @message = attributes['message']
                @timestamp = attributes['timestamp']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'success' => @success,
                  'message' => @message,
                  'timestamp' => @timestamp,
                }
              end
            end
    end
  end
end
