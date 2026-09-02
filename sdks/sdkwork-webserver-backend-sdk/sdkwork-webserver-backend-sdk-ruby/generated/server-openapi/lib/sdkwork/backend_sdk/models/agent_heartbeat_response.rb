module Sdkwork
  module BackendSdk
    module Models
      class AgentHeartbeatResponse
              attr_accessor :server_id, :status, :acknowledged_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @server_id = attributes['serverId']
                @status = attributes['status']
                @acknowledged_at = attributes['acknowledgedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'serverId' => @server_id,
                  'status' => @status,
                  'acknowledgedAt' => @acknowledged_at,
                }
              end
            end
    end
  end
end
