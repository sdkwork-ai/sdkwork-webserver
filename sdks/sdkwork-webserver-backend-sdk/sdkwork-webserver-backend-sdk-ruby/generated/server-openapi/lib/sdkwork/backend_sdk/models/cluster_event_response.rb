module Sdkwork
  module BackendSdk
    module Models
      class ClusterEventResponse
              attr_accessor :id, :cluster_id, :host_id, :instance_id, :event_type, :severity, :message, :detail, :occurred_at, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @cluster_id = attributes['clusterId']
                @host_id = attributes['hostId']
                @instance_id = attributes['instanceId']
                @event_type = attributes['eventType']
                @severity = attributes['severity']
                @message = attributes['message']
                @detail = attributes['detail'].is_a?(Hash) ? attributes['detail'] : {}
                @occurred_at = attributes['occurredAt']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'clusterId' => @cluster_id,
                  'hostId' => @host_id,
                  'instanceId' => @instance_id,
                  'eventType' => @event_type,
                  'severity' => @severity,
                  'message' => @message,
                  'detail' => @detail,
                  'occurredAt' => @occurred_at,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
