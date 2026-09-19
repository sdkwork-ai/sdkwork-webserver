module Sdkwork
  module BackendSdk
    module Models
      class EnqueueClusterPeerMessagesRequest
              attr_accessor :cluster_id, :to_instance_id, :from_instance_id, :message_type, :payload, :expires_in_seconds

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @cluster_id = attributes['clusterId']
                @to_instance_id = attributes['toInstanceId']
                @from_instance_id = attributes['fromInstanceId']
                @message_type = attributes['messageType']
                @payload = attributes['payload'].is_a?(Hash) ? attributes['payload'] : {}
                @expires_in_seconds = attributes['expiresInSeconds']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'clusterId' => @cluster_id,
                  'toInstanceId' => @to_instance_id,
                  'fromInstanceId' => @from_instance_id,
                  'messageType' => @message_type,
                  'payload' => @payload,
                  'expiresInSeconds' => @expires_in_seconds,
                }
              end
            end
    end
  end
end
