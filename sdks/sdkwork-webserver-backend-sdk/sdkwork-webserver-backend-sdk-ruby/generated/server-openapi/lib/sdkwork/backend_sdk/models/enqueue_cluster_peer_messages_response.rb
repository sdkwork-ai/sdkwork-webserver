module Sdkwork
  module BackendSdk
    module Models
      class EnqueueClusterPeerMessagesResponse
              attr_accessor :enqueued

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @enqueued = attributes['enqueued']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'enqueued' => @enqueued,
                }
              end
            end
    end
  end
end
