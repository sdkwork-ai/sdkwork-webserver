module Sdkwork
  module BackendSdk
    module Models
      class SdkWorkAsyncData
              attr_accessor :accepted, :operation_id, :status, :poll_url

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @accepted = attributes['accepted']
                @operation_id = attributes['operationId']
                @status = attributes['status']
                @poll_url = attributes['pollUrl']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'accepted' => @accepted,
                  'operationId' => @operation_id,
                  'status' => @status,
                  'pollUrl' => @poll_url,
                }
              end
            end
    end
  end
end
