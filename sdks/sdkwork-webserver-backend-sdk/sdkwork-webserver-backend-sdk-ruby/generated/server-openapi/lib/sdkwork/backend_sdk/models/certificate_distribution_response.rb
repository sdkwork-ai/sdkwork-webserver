module Sdkwork
  module BackendSdk
    module Models
      class CertificateDistributionResponse
              attr_accessor :server_id, :server_name, :host, :desired_sync_version, :applied_sync_version, :status, :last_heartbeat_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @server_id = attributes['serverId']
                @server_name = attributes['serverName']
                @host = attributes['host']
                @desired_sync_version = attributes['desiredSyncVersion']
                @applied_sync_version = attributes['appliedSyncVersion']
                @status = attributes['status']
                @last_heartbeat_at = attributes['lastHeartbeatAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'serverId' => @server_id,
                  'serverName' => @server_name,
                  'host' => @host,
                  'desiredSyncVersion' => @desired_sync_version,
                  'appliedSyncVersion' => @applied_sync_version,
                  'status' => @status,
                  'lastHeartbeatAt' => @last_heartbeat_at,
                }
              end
            end
    end
  end
end
